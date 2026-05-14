mod config;
mod socket;

use monoio::net::TcpListener;
use socket2::{Domain, Socket, Type};
use std::os::unix::io::AsRawFd;

fn main() {
    let cfg = config::proxy_config();

    let handles: Vec<_> = (0..cfg.workers)
        .map(|_| {
            let upstreams = cfg.upstreams.clone();
            let port = cfg.port;
            std::thread::spawn(move || {
                let ctrl_fds: Vec<libc::c_int> = upstreams
                    .iter()
                    .map(|path| socket::connect_ctrl(&format!("{path}.ctrl")))
                    .collect();

                monoio::RuntimeBuilder::<monoio::IoUringDriver>::new()
                    .with_entries(4096)
                    .build()
                    .expect("failed to build IoUring runtime")
                    .block_on(accept_loop(port, ctrl_fds))
            })
        })
        .collect();

    for h in handles {
        h.join().expect("worker thread panicked");
    }
}

fn make_listener(port: u16) -> std::net::TcpListener {
    let sock = Socket::new(Domain::IPV4, Type::STREAM, None).expect("socket2::Socket::new");
    sock.set_reuse_address(true).expect("SO_REUSEADDR");
    sock.set_reuse_port(true).expect("SO_REUSEPORT");
    sock.set_nonblocking(true).expect("O_NONBLOCK");
    let addr: std::net::SocketAddr = format!("0.0.0.0:{port}").parse().unwrap();
    sock.bind(&addr.into()).expect("bind");
    sock.listen(65535).expect("listen");
    sock.into()
}

async fn accept_loop(port: u16, ctrl_fds: Vec<libc::c_int>) {
    let listener = TcpListener::from_std(make_listener(port)).expect("TcpListener::from_std");
    let len = ctrl_fds.len();
    let mut rr: usize = 0;
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                stream.set_nodelay(true).ok();
                let ctrl = ctrl_fds[rr % len];
                rr = rr.wrapping_add(1);
                unsafe { socket::send_fd(ctrl, stream.as_raw_fd()) };
            }
            Err(_) => {}
        }
    }
}
