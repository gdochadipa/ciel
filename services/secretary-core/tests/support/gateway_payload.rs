//! Independent synthetic ELF: assert the kernel authority after the real launcher exec.
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
};

fn main() {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(unsafe { libc::getuid() }, 65534);
    assert_eq!(unsafe { libc::geteuid() }, 65534);
    assert_eq!(unsafe { libc::getgid() }, 65534);
    assert_eq!(unsafe { libc::getegid() }, 65534);
    assert_eq!(unsafe { libc::getgroups(0, std::ptr::null_mut()) }, 0);
    assert_eq!(
        unsafe { libc::prctl(libc::PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0) },
        1
    );
    // Close the enumeration handle before checking the numbers it observed.
    let descriptors: Vec<i32> = std::fs::read_dir("/proc/thread-self/fd")
        .unwrap()
        .map(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    for descriptor in descriptors.into_iter().filter(|fd| *fd > 2) {
        assert_eq!(
            unsafe { libc::fcntl(descriptor, libc::F_GETFD) },
            -1,
            "unexpected inherited handle"
        );
    }
    let status = std::fs::read_to_string("/proc/thread-self/status").unwrap();
    for name in ["CapInh", "CapPrm", "CapEff", "CapBnd", "CapAmb"] {
        assert!(status
            .lines()
            .any(|line| line == format!("{name}:\t0000000000000000")));
    }
    for name in ["Uid", "Gid"] {
        let line = status
            .lines()
            .find(|line| line.starts_with(&format!("{name}:")))
            .unwrap();
        assert_eq!(
            line.split_once(':')
                .unwrap()
                .1
                .split_ascii_whitespace()
                .collect::<Vec<_>>(),
            vec!["65534"; 4]
        );
    }
    for path in &arguments[1..] {
        let error = std::fs::File::open(path).unwrap_err();
        assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    }
    assert!(std::env::var_os("ROOT_ENV_CANARY").is_none());
    assert!(std::env::var_os("SECRETARY_CORE_TOKEN").is_none());
    assert_eq!(std::env::var("LANG").unwrap(), "C");
    let (release, wait) = mpsc::channel::<()>();
    let (ready, started) = mpsc::channel();
    let worker = (arguments[0] == "gateway-payload-threaded").then(|| {
        thread::spawn(move || {
            ready.send(()).unwrap();
            let _ = wait.recv();
        })
    });
    if worker.is_some() {
        started.recv().unwrap();
    }
    println!("gateway_ready");
    std::io::stdout().flush().unwrap();
    std::io::stdin().read_exact(&mut [0]).unwrap();
    drop(release);
    if let Some(worker) = worker {
        worker.join().unwrap();
    }
}
