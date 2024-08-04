use crate::Error;
use libc::{uname, utsname, IFF_MULTI_QUEUE, IFF_NO_PI, IFF_TAP, IFF_TUN};
use log::warn;
use netconfig::sys::posix::ifreq::ifreq;
use std::fs;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use tunio_core::config::Layer;

mod ioctls {
    nix::ioctl_write_int!(tunsetiff, b'T', 202);
    nix::ioctl_write_int!(tunsetpersist, b'T', 203);
    nix::ioctl_write_int!(tunsetowner, b'T', 204);
    nix::ioctl_write_int!(tunsetgroup, b'T', 206);
}

pub(crate) struct Device {
    pub device: fs::File,
    pub name: String,
}

pub(crate) fn create_device(name: &str, layer: Layer, blocking: bool) -> Result<Device, Error> {
    let mut open_opts = fs::OpenOptions::new();
    open_opts.read(true).write(true);
    if !blocking {
        open_opts.custom_flags(libc::O_NONBLOCK);
    }
    let tun_device = open_opts.open("/dev/net/tun")?;

    let mut init_flags = match layer {
        Layer::L2 => IFF_TAP,
        Layer::L3 => IFF_TUN,
    };
    init_flags |= IFF_NO_PI;

    // https://www.kernel.org/doc/html/v5.12/networking/tuntap.html#multiqueue-tuntap-interface
    // From version 3.8, Linux supports multiqueue tuntap which can uses multiple file descriptors (queues) to parallelize packets sending or receiving.
    // check if linux kernel version is 3.8+ and set IFF_MUTLI_QUEUE
    if check_if_multiqueue_support() {
        init_flags |= IFF_MULTI_QUEUE;
    }

    let mut req = ifreq::new(name);
    req.ifr_ifru.ifru_flags = init_flags as _;

    unsafe { ioctls::tunsetiff(tun_device.as_raw_fd(), &req as *const _ as _) }.unwrap();

    // Name can change due to formatting
    Ok(Device {
        device: tun_device,
        name: String::try_from(req.ifr_ifrn)
            .map_err(|e| Error::InterfaceNameError(format!("{e:?}")))?,
    })
}

fn check_if_multiqueue_support() -> bool {
    unsafe {
        let mut uname_buf: std::mem::MaybeUninit<utsname> =
            std::mem::MaybeUninit::<utsname>::zeroed();
        uname(uname_buf.as_mut_ptr());

        let uname_data = uname_buf.assume_init();
        let uname_str =
            std::str::from_utf8_unchecked(std::mem::transmute(&uname_data.release as &[i8]));

        let mut version = uname_str.split(".");

        match (
            version.next().and_then(|s| s.parse::<usize>().ok()),
            version.next().and_then(|s| s.parse::<usize>().ok()),
        ) {
            (Some(major), Some(minor)) if major > 3 || (major == 3 && minor >= 8) => {
                return true;
            }
            _ => {
                warn!(
                    "Kernel doesn't support Multique TUN interface (must be Linux Kernel >= 3.8, current: {uname_str:?})"
                );
                return false;
            }
        }
    }
}
