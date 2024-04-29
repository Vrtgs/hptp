use ahash::HashSetExt;
use std::net::{IpAddr, SocketAddr};

#[derive(Debug, Copy, Clone)]
pub enum FilterMode {
    BlackList,
    WhiteList,
}

mod sealed {
    pub trait FilterInner {
        type FilterType;
    }
}

pub type Filter<T> = <T as sealed::FilterInner>::FilterType;

macro_rules! filter {
    ($ty: ty) => {paste::paste! {
        #[allow(non_snake_case)]
        mod [<__sealed_filter_ $ty>] {
            use ahash::HashSet;
            use arrayvec::ArrayVec;

            use super::*;

            pub const SMALL_LEN: usize = {
                use std::mem::size_of;

                let len_marker = size_of::<u32>();
                let array_item = size_of::<$ty>();
                let val = ((size_of::<HashSet<$ty>>().saturating_sub(len_marker)) / array_item);
                if val < 8 {
                    8
                } else {
                    val
                }
            };

            #[derive(Debug, Clone)]
            enum FilterRepr {
                Small(ArrayVec<$ty, SMALL_LEN>),
                Big(HashSet<$ty>),
            }

            impl FilterRepr {
                fn add(&mut self, items: &[$ty]) {
                    match self {
                        FilterRepr::Small(arr) => match arr.try_extend_from_slice(items) {
                            Ok(()) => {
                                arr.sort_unstable();
                                let mut prev = None;
                                arr.retain(|&mut x| {
                                    if prev.map_or(false, |prev| prev == x) {
                                        return false;
                                    }
                                    prev = Some(x);
                                    true
                                });
                            }
                            Err(_) => *self = FilterRepr::Big(arr.iter().chain(items).copied().collect()),
                        },
                        FilterRepr::Big(map) => map.extend(items),
                    }
                }

                fn shrink(&mut self) {
                    if let FilterRepr::Big(map) = self {
                        if map.len() < SMALL_LEN {
                            *self = FilterRepr::Small(ArrayVec::from_iter(map.iter().copied()))
                        }
                    }
                }

                fn contains(&self, item: $ty) -> bool {
                    match self {
                        FilterRepr::Small(arr) => arr.binary_search(&item).is_ok(),
                        FilterRepr::Big(set) => set.contains(&item)
                    }
                }
            }

            #[derive(Debug, Clone)]
            pub struct Filter {
                repr: FilterRepr,
                mode: FilterMode
            }

            impl Filter {
                pub fn new(items: &[$ty], mode: FilterMode) -> Self {
                    let mut repr = match items.len() > SMALL_LEN {
                        true => FilterRepr::Big(HashSet::with_capacity(items.len())),
                        false => FilterRepr::Small(ArrayVec::new())
                    };

                    repr.add(items);
                    repr.shrink();

                    Self { repr, mode }
                }

                pub fn allowed(&self, item: $ty) -> bool {
                    let contains = self.repr.contains(item);
                    let flip = match self.mode {
                        FilterMode::BlackList => true,
                        FilterMode::WhiteList => false
                    };

                    contains ^ flip
                }
            }
        }

        impl sealed::FilterInner for $ty {
            type FilterType = [<__sealed_filter_ $ty>]::Filter;
        }
    }};
}

pub type Port = u16;

filter! { Port }
filter! { IpAddr }

mod __sealed_filter_socket {
    use super::*;

    #[derive(Debug, Clone)]
    pub struct SocketAddrFilter {
        ip_filter: Filter<IpAddr>,
        port_filter: Filter<Port>,
    }

    impl SocketAddrFilter {
        pub fn new(ip_filter: Filter<IpAddr>, port_filter: Filter<Port>) -> Self {
            SocketAddrFilter {
                ip_filter,
                port_filter,
            }
        }
        pub fn allowed(&self, addr: SocketAddr) -> bool {
            self.port_filter.allowed(addr.port()) && self.ip_filter.allowed(addr.ip())
        }
    }
}

impl sealed::FilterInner for SocketAddr {
    type FilterType = __sealed_filter_socket::SocketAddrFilter;
}
