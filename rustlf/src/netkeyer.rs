#![allow(unused)]

use std::cell::RefCell;
use std::error::Error;
use std::ffi::{c_char, c_uint, c_void, CStr, CString};
use std::io::{Cursor, Write};
use std::net::{
    Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs, UdpSocket,
};
use std::ops::Deref;
use std::sync::Arc;

use crate::cw_utils::GetCWSpeed;
use crate::err_utils::{log_message, LogLevel};
use crate::{parse_cstr, tlf};

thread_local! {
    pub(crate) static NETKEYER: RefCell<Arc<Option<Netkeyer>>> = RefCell::new(Arc::new(None));
}

pub(crate) struct Netkeyer {
    socket: UdpSocket,
    dest_addr: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum KeyerError {
    #[error("IO")]
    IO(#[from] std::io::Error),
    #[error("Invalid parameter supplied")]
    InvalidParameter,
}

pub(crate) trait TextKeyer: Send {
    fn send_text(&mut self, text: &[u8]);
}

const ESC: u8 = 0x1b;

fn make_buf<const N: usize>() -> Cursor<[u8; N]> {
    Cursor::new([0; N])
}

fn extract_buf<'a, const N: usize>(cursor: &'a Cursor<[u8; N]>) -> &'a [u8] {
    let s = cursor.get_ref().as_slice();
    &s[..cursor.position() as usize]
}

macro_rules! write_esc {
    ($buf:expr,$fmt:expr,$value:expr) => {
        write!($buf, concat!("\x1b", $fmt), $value);
    };
}

impl Netkeyer {
    pub(crate) fn new(dest_addr: SocketAddr) -> std::io::Result<Netkeyer> {
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

        Ok(Netkeyer { socket, dest_addr })
    }

    pub(crate) fn from_host_and_port(host: &str, port: u16) -> std::io::Result<Netkeyer> {
        let dest_addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or(std::io::ErrorKind::NotFound)?;

        Netkeyer::new(dest_addr)
    }

    #[inline]
    fn simple_command(&self, cmd: u8) -> Result<(), KeyerError> {
        let cmd = [ESC, cmd];
        let _ = self.socket.send_to(cmd.as_ref(), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn reset(&self) -> Result<(), KeyerError> {
        self.simple_command(b'0')
    }

    pub(crate) fn set_speed(&self, speed: u8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<4>();

        if speed < 5 || speed > 60 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "2{}", speed);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_tone(&self, tone: u16) -> Result<(), KeyerError> {
        let mut buf = make_buf::<6>();

        if tone != 0 && (tone < 300 || tone > 1000) {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "3{}", tone);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn abort(&self) -> Result<(), KeyerError> {
        self.simple_command(b'4')
    }

    pub(crate) fn stop(&self) -> Result<(), KeyerError> {
        self.simple_command(b'5')
    }

    pub(crate) fn enable_word_mode(&self) -> Result<(), KeyerError> {
        self.simple_command(b'6')
    }

    pub(crate) fn set_weight(&self, weight: i8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<6>();

        if weight < -50 || weight > 50 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "7{}", weight);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_device(&self, device: &[u8]) -> Result<(), KeyerError> {
        let mut buf = Vec::with_capacity(device.len() + 2);
        buf.push(ESC);
        buf.push(b'8');
        buf.extend_from_slice(device);

        let _ = self.socket.send_to(&buf, self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_ptt(&self, ptt: bool) -> Result<(), KeyerError> {
        let mut buf = make_buf::<3>();
        write_esc!(buf, "a{}", ptt as u8);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_pin14(&self, pin14: bool) -> Result<(), KeyerError> {
        let mut buf = make_buf::<3>();
        write_esc!(buf, "b{}", pin14 as u8);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn tune(&self, seconds: u8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<4>();

        if seconds > 10 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "c{}", seconds);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_tx_delay(&self, ms: u8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<4>();

        if ms > 50 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "d{}", ms);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_band_switch(&self, bandindex: u8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<4>();

        if bandindex < 1 || bandindex > 9 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "e{}", bandindex);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_sidetone_device(&self, dev: u8) -> Result<(), KeyerError> {
        let cmd = [ESC, b'f', dev];

        if !b"coapns".contains(&dev) {
            return Err(KeyerError::InvalidParameter);
        }

        let _ = self.socket.send_to(cmd.as_ref(), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_sidetone_volume(&self, volume: u8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<6>();

        if volume > 100 {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "g{}", volume);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn send_text(&self, text: &[u8]) -> Result<(), KeyerError> {
        if text.contains(&ESC) {
            return Err(KeyerError::InvalidParameter);
        }

        let _ = self.socket.send_to(text, self.dest_addr)?;
        Ok(())
    }
}

impl TextKeyer for Netkeyer {
    fn send_text(&mut self, text: &[u8]) {}
}
#[no_mangle]
pub unsafe extern "C" fn write_tone() {
    let netkeyer = NETKEYER.with(|netkeyer| {
        if let Some(netkeyer) = netkeyer.borrow().deref().as_ref() {
            write_tone_inner(netkeyer).expect("netkeyer send error");
        }
    });
}

pub(crate) unsafe fn write_tone_inner(netkeyer: &Netkeyer) -> Result<(), KeyerError> {
    let tonestr = CStr::from_ptr(&tlf::tonestr as *const c_char);
    let tone: Option<u16> = parse_cstr(&tlf::tonestr as *const c_char);

    if tonestr.to_bytes().is_empty() || tone.is_none() {
        return Ok(());
    }
    let tone = tone.unwrap();

    if let Err(_) = netkeyer.set_tone(tone) {
        log_message(
            LogLevel::INFO,
            CStr::from_bytes_with_nul(b"keyer not active; switching to SSB\x00").unwrap(),
        );
        tlf::trxmode = tlf::SSBMODE as _;
        return Ok(());
    }

    if tone != 0 {
        /* work around bugs in cwdaemon:
         * cwdaemon < 0.9.6 always set volume to 70% at change of tone freq
         * cwdaemon >=0.9.6 do not set volume at all after change of freq,
         * resulting in no tone output if you have a freq=0 in between
         * So... to be sure we set the volume back to our chosen value
         * or to 70% (like cwdaemon) if no volume got specified
         */
        let sc_volume: u8 = parse_cstr(&tlf::sc_volume as *const c_char).unwrap_or(70);

        netkeyer.set_sidetone_volume(sc_volume);
    }
    Ok(())
}

pub(crate) unsafe fn init_params(netkeyer: &Netkeyer) -> Result<(), KeyerError> {
    netkeyer.reset()?;
    netkeyer.set_weight(tlf::weight as i8)?;

    write_tone_inner(netkeyer)?;

    netkeyer.set_speed(GetCWSpeed().try_into().unwrap())?;
    netkeyer.set_weight(tlf::weight as _)?;

    let keyer_device = CStr::from_ptr(&tlf::keyer_device as *const c_char);

    if !keyer_device.to_bytes().is_empty() {
        netkeyer.set_device(keyer_device.to_bytes())?;
    }

    netkeyer.set_tx_delay(tlf::txdelay as _)?;
    if tlf::sc_sidetone {
        netkeyer.set_sidetone_device(b's')?;
    }

    let sc_volume = CStr::from_ptr(&tlf::sc_volume as *const c_char);
    if !sc_volume.to_bytes().is_empty() {
        netkeyer.set_sidetone_volume(parse_cstr(&tlf::sc_volume as *const c_char).unwrap())?;
    }

    Ok(())
}
