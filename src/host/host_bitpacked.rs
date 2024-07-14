use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::net::IpAddr;
use std::ptr::NonNull;

use hickory_resolver::Name;

use crate::dns_resolver::DnsResolver;

#[repr(align(2))]
pub(super) struct DynamicHost {
    pub(super) name: Name,
    pub(super) resolver: DnsResolver,
}

#[derive(Copy, Clone)]
#[repr(align(2))]
pub(super) struct AlignedIp(pub(super) IpAddr);

#[derive(Copy, Clone)]
pub(super) enum HostRpr {
    Static(&'static IpAddr),
    Dynamic(&'static DynamicHost),
}

const TAG_MASK: usize = 0b1;
const IP_TAG: usize = 0b1;
const DYN_HOST_TAG: usize = 0b0;

union HostPointsTo {
    // tag bit: 1
    ip: AlignedIp,
    // tag bit: 0
    dynamic_host: ManuallyDrop<DynamicHost>,
}

#[derive(Copy, Clone)]
pub struct Host {
    ptr: NonNull<HostPointsTo>,
    _marker: PhantomData<HostRpr>,
}
// Asserted bellow that the `HostRpr` that `Host` stores internally is Send + Sync, and so is it.
unsafe impl Send for Host {}
unsafe impl Sync for Host {}

impl Host {
    #[inline(always)]
    unsafe fn as_ip_ref(&self) -> &'static IpAddr {
        const { assert!(IP_TAG <= std::mem::size_of::<AlignedIp>()) }

        // Note: We know `IP_TAG <= size_of::<AlignedIp>()`,
        // and this function will only be called on instances of self where IP_TAG is set
        // and both the start and end of the expression must be
        // valid without address space wraparound due to how references work
        // This means it would be correct to implement this using `ptr::byte_sub`
        unsafe { &(*self.ptr.as_ptr().byte_sub(IP_TAG)).ip.0 }
    }

    #[inline(always)]
    unsafe fn as_dyn_host_ref(&self) -> &'static DynamicHost {
        // rust-rover complains that assert!(x == y) can be replaced with assert_eq
        // this isn't true as where in a const setting and u cant call assert_eq in const
        const { assert!(matches!(DYN_HOST_TAG, 0)) }

        // Note: We know DYN_HOST_TAG == 0, so it doesn't affect the pointer at all
        unsafe { &self.ptr.as_ref().dynamic_host }
    }

    pub(super) fn as_repr(self) -> HostRpr {
        // TODO: strict provence
        let bits = self.ptr.as_ptr() as usize;
        match bits & TAG_MASK {
            IP_TAG => HostRpr::Static(unsafe { self.as_ip_ref() }),
            DYN_HOST_TAG => HostRpr::Dynamic(unsafe { self.as_dyn_host_ref() }),
            _ => {
                // Can't happen, and compiler can tell
                unreachable!();
            }
        }
    }
}

impl From<&'static AlignedIp> for Host {
    fn from(value: &'static AlignedIp) -> Self {
        const { assert!(IP_TAG <= std::mem::size_of::<AlignedIp>()) }

        // Note: We know `IP_TAG <= size_of::<AlignedIp>()`,
        // and this function will only be called on instances of self where IP_TAG is set
        // and both the start and end of the expression must be
        // valid without address space wraparound due to how references work
        // This means it would be correct to implement this using `ptr::byte_add`

        let ptr = unsafe { (value as *const _ as *const HostPointsTo).byte_add(1) };
        Self {
            // Safety: references are always non-null
            ptr: unsafe { NonNull::new_unchecked(ptr as *mut _) },
            _marker: PhantomData,
        }
    }
}

impl From<&'static DynamicHost> for Host {
    fn from(value: &'static DynamicHost) -> Self {
        Self {
            // Note: We know DYN_HOST_TAG == 0, so it doesn't affect the pointer at all
            // so, we can just cast the type
            ptr: NonNull::from(value).cast(),
            _marker: PhantomData,
        }
    }
}

fn _assert() {
    fn send_sync<T: Send + Sync>() {}

    send_sync::<HostRpr>()
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;
    use std::str::FromStr;
    use std::sync::OnceLock;

    use super::*;

    #[test]
    fn ip_works() {
        macro_rules! test_ip {
            ($ip: expr) => {{
                const IP: IpAddr = $ip;
                assert!(matches!(
                    Host::from(&AlignedIp(IP)).as_repr(),
                    HostRpr::Static(&IP)
                ))
            }};
        }

        test_ip!(IpAddr::V6(Ipv6Addr::LOCALHOST));
        test_ip!(IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        test_ip!(IpAddr::V6(Ipv6Addr::new(1, 1, 1, 1, 1, 1, 1, 1)));
        test_ip!(IpAddr::V6(Ipv6Addr::new(12, 1, 221, 91, 61, 881, 1, 8881)));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn name_works() {
        macro_rules! test_host {
            ($name: expr) => {{
                static HOST: OnceLock<DynamicHost> = OnceLock::new();
                let name = Name::from_str($name).unwrap();
                let host = Host::from(HOST.get_or_init(|| DynamicHost {
                    name: name.clone(),
                    resolver: DnsResolver::default(),
                }));
                let HostRpr::Dynamic(host) = host.as_repr() else {
                    unreachable!()
                };
                assert_eq!(host.name, name);
            }};
        }

        test_host!("example.com");
        test_host!("foo.bar");
        test_host!("vrtgs.xyz");
        test_host!("www.youtube.com");
        test_host!("docs.rs");
    }
}
