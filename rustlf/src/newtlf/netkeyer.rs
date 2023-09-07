use std::io::{Cursor, Write};
use std::net::{
    Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs, UdpSocket,
};
use std::sync::atomic::{AtomicI8, Ordering};

pub(crate) struct Netkeyer {
    socket: UdpSocket,
    dest_addr: SocketAddr,
    sc_volume: AtomicI8,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO")]
    IO(#[from] std::io::Error),
    #[error("Invalid parameter supplied")]
    InvalidParameter,
    #[error("Invalid device")]
    InvalidDevice,
}

const ESC: u8 = 0x1b;

fn make_buf<const N: usize>() -> Cursor<[u8; N]> {
    Cursor::new([0; N])
}

fn extract_buf<const N: usize>(cursor: &Cursor<[u8; N]>) -> &[u8] {
    let s = cursor.get_ref().as_slice();
    &s[..cursor.position() as usize]
}

macro_rules! write_esc {
    ($buf:expr,$fmt:expr,$value:expr) => {
        write!($buf, concat!("\x1b", $fmt), $value).expect("buffer write errror");
    };
}

impl Netkeyer {
    pub(crate) fn new(dest_addr: SocketAddr) -> Result<Netkeyer, Error> {
        let bind_addr: SocketAddr = match dest_addr {
            SocketAddr::V4(dest) => {
                if dest.ip().is_loopback() {
                    SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into()
                } else {
                    SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0).into()
                }
            }
            SocketAddr::V6(dest) => {
                if dest.ip().is_loopback() {
                    SocketAddrV6::new(Ipv6Addr::LOCALHOST, 0, 0, 0).into()
                } else {
                    SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, 0).into()
                }
            }
        };

        let socket = UdpSocket::bind(bind_addr)?;

        Ok(Netkeyer {
            socket,
            dest_addr,
            sc_volume: AtomicI8::new(-1),
        })
    }

    pub(crate) fn from_host_and_port(host: &str, port: u16) -> Result<Netkeyer, Error> {
        let dest_addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or(std::io::Error::from(std::io::ErrorKind::NotFound))?;

        Netkeyer::new(dest_addr)
    }

    pub(crate) fn write_tone(&self, tone: u16) -> Result<(), Error> {
        self.set_tone(tone)?;

        if tone != 0 {
            /* work around bugs in cwdaemon:
             * cwdaemon < 0.9.6 always set volume to 70% at change of tone freq
             * cwdaemon >=0.9.6 do not set volume at all after change of freq,
             * resulting in no tone output if you have a freq=0 in between
             * So... to be sure we set the volume back to our chosen value
             * or to 70% (like cwdaemon) if no volume got specified
             */
            let sc_volume: u8 = Some(self.sc_volume.load(Ordering::Acquire))
                .and_then(|v| v.try_into().ok())
                .unwrap_or(70);
            self.set_sidetone_volume(sc_volume)?;
        }
        Ok(())
    }

    #[inline]
    fn simple_command(&self, cmd: u8) -> Result<(), Error> {
        let cmd = [ESC, cmd];
        let _ = self.socket.send_to(cmd.as_ref(), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn reset(&self) -> Result<(), Error> {
        self.simple_command(b'0')
    }

    pub(crate) fn set_speed(&self, speed: u8) -> Result<(), Error> {
        let mut buf = make_buf::<4>();

        if !(5..=60).contains(&speed) {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "2{}", speed);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_tone(&self, tone: u16) -> Result<(), Error> {
        let mut buf = make_buf::<6>();

        if tone != 0 && !(300..=1000).contains(&tone) {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "3{}", tone);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn abort(&self) -> Result<(), Error> {
        self.simple_command(b'4')
    }

    #[allow(unused)]
    pub(crate) fn exit(&self) -> Result<(), Error> {
        self.simple_command(b'5')
    }

    pub(crate) fn enable_word_mode(&self) -> Result<(), Error> {
        self.simple_command(b'6')
    }

    pub(crate) fn set_weight(&self, weight: i8) -> Result<(), Error> {
        let mut buf = make_buf::<6>();

        if !(-50..=50).contains(&weight) {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "7{}", weight);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_device(&self, device: &[u8]) -> Result<(), Error> {
        let mut buf = Vec::with_capacity(device.len() + 2);
        buf.push(ESC);
        buf.push(b'8');
        buf.extend_from_slice(device);

        let _ = self.socket.send_to(&buf, self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_ptt(&self, ptt: bool) -> Result<(), Error> {
        let mut buf = make_buf::<3>();
        write_esc!(buf, "a{}", ptt as u8);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_pin14(&self, pin14: bool) -> Result<(), Error> {
        let mut buf = make_buf::<3>();
        write_esc!(buf, "b{}", pin14 as u8);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn tune(&self, seconds: u8) -> Result<(), Error> {
        let mut buf = make_buf::<4>();

        if seconds > 10 {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "c{}", seconds);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_tx_delay(&self, ms: u8) -> Result<(), Error> {
        let mut buf = make_buf::<4>();

        if ms > 50 {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "d{}", ms);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_band_switch(&self, bandindex: u8) -> Result<(), Error> {
        let mut buf = make_buf::<4>();

        if !(1..=9).contains(&bandindex) {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "e{}", bandindex);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_sidetone_device(&self, dev: u8) -> Result<(), Error> {
        let cmd = [ESC, b'f', dev];

        if !b"coapns".contains(&dev) {
            return Err(Error::InvalidParameter);
        }

        let _ = self.socket.send_to(cmd.as_ref(), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_sidetone_volume(&self, volume: u8) -> Result<(), Error> {
        self.sc_volume
            .store(volume.try_into().ok().unwrap_or(-1), Ordering::Release);
        let mut buf = make_buf::<6>();

        if volume > 100 {
            return Err(Error::InvalidParameter);
        }

        write_esc!(buf, "g{}", volume);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn send_text(&self, text: &[u8]) -> Result<(), Error> {
        if text.contains(&ESC) {
            return Err(Error::InvalidParameter);
        }

        let _ = self.socket.send_to(text, self.dest_addr)?;
        Ok(())
    }
}
