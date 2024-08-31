use itertools::Itertools;
use std::fmt::{Display, Formatter};
use std::iter::FusedIterator;
use std::num::NonZero;
use std::str::FromStr;

#[derive(thiserror::Error, Debug)]
#[error(
    "invalid ports array, \
    expected [$(<ELM>),+] where <ELM> can be a port number, \
    inclusive range x..y, or an exclusive range x..!=y\
    "
)]
#[cfg_attr(test, derive(PartialEq))]
pub struct PortsArrayParseError(());

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, Ord, PartialOrd, Eq, PartialEq))]
pub struct PortsArray(Vec<NonZero<u16>>);

impl PortsArray {
    pub fn into_ports_vec(self) -> Vec<NonZero<u16>> {
        self.0
    }
}

impl FromStr for PortsArray {
    type Err = PortsArrayParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.trim()
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .and_then(|s| {
                enum OneOrRange<T> {
                    One(T),
                    RangeInclusive(T, T, bool),
                    RangeExclusive(T, T),
                }
                use OneOrRange::*;

                impl Iterator for OneOrRange<NonZero<u16>> {
                    type Item = NonZero<u16>;

                    fn next(&mut self) -> Option<Self::Item> {
                        match self {
                            One(one) => {
                                // empty
                                let one = *one;
                                *self = RangeExclusive(NonZero::<u16>::MAX, NonZero::<u16>::MAX);
                                Some(one)
                            }
                            RangeInclusive(start, end, exhausted) => {
                                if start > end {
                                    None
                                } else if start == end && !*exhausted {
                                    *exhausted = true;
                                    Some(*start)
                                } else {
                                    let tmp = *start;
                                    *start = start.checked_add(1)?;
                                    Some(tmp)
                                }
                            }
                            RangeExclusive(start, end) => {
                                if start >= end {
                                    None
                                } else {
                                    let tmp = *start;
                                    *start = start.checked_add(1)?;
                                    Some(tmp)
                                }
                            }
                        }
                    }
                }
                impl FusedIterator for OneOrRange<NonZero<u16>> {}

                let array = s
                    .split(',')
                    .map(str::trim)
                    .map(|s| {
                        s.parse::<NonZero<u16>>().ok().map(One).or_else(|| {
                            let parse_range =
                                |(s1, s2): (&str, &str)| Some((s1.parse().ok()?, s2.parse().ok()?));
                            s.split_once("..")
                                .and_then(parse_range)
                                .map(|(x, y)| RangeInclusive(x, y, false))
                                .or_else(|| {
                                    s.split_once("..!=")
                                        .and_then(parse_range)
                                        .map(|(x, y)| RangeExclusive(x, y))
                                })
                        })
                    })
                    .collect::<Option<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .unique()
                    .sorted()
                    .collect::<Vec<NonZero<u16>>>();

                Some(array)
            })
            .filter(|x| !x.is_empty())
            .or_else(|| Some(vec![s.parse().ok()?]))
            .ok_or(PortsArrayParseError(()))
            .map(PortsArray)
    }
}

impl Display for PortsArray {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut list_dbg = f.debug_list();

        let mut ports = self.0.iter().copied();
        while let Some(start) = ports.next() {
            let mut end = start;

            macro_rules! peek_end {
                () => {
                    (|| {
                        let new_end = end.checked_add(1)?;
                        (new_end == ports.clone().next()?).then_some(new_end)
                    })()
                };
            }

            while let Some(new_end) = peek_end!() {
                end = new_end;
                ports.next();
            }

            if (end.get() - start.get()) < 2 {
                for i in start.get()..=end.get() {
                    list_dbg.entry(&i);
                }
            } else {
                list_dbg.entry(&format_args!("{}..{}", start, end));
            }
        }

        list_dbg.finish()
    }
}

#[cfg(test)]
mod test_port_array {
    use super::*;

    fn non_zero(x: u16) -> NonZero<u16> {
        NonZero::new(x).unwrap()
    }

    #[test]
    // too slow on miri, and there is no unsafe to test
    #[cfg_attr(miri, ignore)]
    fn test_valid_ports_array() {
        // Test valid input strings and expected PortsArray values
        assert_eq!(
            "[80, 443, 20..24, 8080]".parse::<PortsArray>(),
            Ok(PortsArray(
                [80, 443]
                    .into_iter()
                    .chain(20..=24)
                    .chain([8080])
                    .map(non_zero)
                    .unique()
                    .sorted()
                    .collect()
            ))
        );
        assert_eq!(
            format!("[1..{}]", u16::MAX).parse::<PortsArray>(),
            Ok(PortsArray((1..=u16::MAX).map(non_zero).collect()))
        );
        assert_eq!(
            format!("[1..!={}]", u16::MAX).parse::<PortsArray>(),
            Ok(PortsArray((1..u16::MAX).map(non_zero).collect()))
        );
    }

    #[test]
    fn test_invalid_ports_array() {
        // Test invalid input strings
        assert!("[]".parse::<PortsArray>().is_err());
        assert!("[80, 443, abc]".parse::<PortsArray>().is_err());
        assert!("[80..=abc]".parse::<PortsArray>().is_err());
        assert!("[80..!=100..=200]".parse::<PortsArray>().is_err());
    }

    #[test]
    fn test_ports_array_display() {
        assert_eq!(
            PortsArray((1..=u16::MAX).map(non_zero).collect()).to_string(),
            format!("[1..{}]", u16::MAX)
        );

        assert_eq!(
            "[80, 81, 82, 443, 8080, 8081, 8082, 8083]"
                .parse::<PortsArray>()
                .unwrap()
                .to_string(),
            "[80..82, 443, 8080..8083]"
        );

        assert_eq!(
            "[80, 81, 443, 444, 8080, 8081, 8082, 8083]"
                .parse::<PortsArray>()
                .unwrap()
                .to_string(),
            "[80, 81, 443, 444, 8080..8083]"
        )
    }
}
