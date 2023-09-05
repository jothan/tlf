use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_uint, CStr};
use std::io::{Cursor, Write};
use std::net::{
    Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs, UdpSocket,
};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use crate::cw_utils::GetCWSpeed;
use crate::err_utils::CResult;
use crate::keyer_interface::{CwKeyerBackend, CwKeyerFrontend};

thread_local! {
    pub(crate) static NETKEYER: RefCell<Option<Arc<Netkeyer>>> = RefCell::new(None);
}

// Could be owned by the main thread if the simulator did not set it.
static TONE: AtomicI32 = AtomicI32::new(600);

pub(crate) struct Netkeyer {
    socket: UdpSocket,
    dest_addr: SocketAddr,
}

#[derive(Debug, thiserror::Error)]
pub enum KeyerError {
    #[error("IO")]
    IO(#[from] std::io::Error),
    #[error("Invalid parameter supplied")]
    InvalidParameter,
    #[error("Invalid device")]
    InvalidDevice,
}

pub(crate) trait TextKeyer: Send {
    fn send_text(&mut self, text: &[u8]);
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
    pub(crate) fn new(dest_addr: SocketAddr) -> Result<Netkeyer, KeyerError> {
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

    pub(crate) fn from_host_and_port(host: &str, port: u16) -> Result<Netkeyer, KeyerError> {
        let dest_addr = (host, port)
            .to_socket_addrs()?
            .next()
            .ok_or(std::io::Error::from(std::io::ErrorKind::NotFound))?;

        Netkeyer::new(dest_addr)
    }

    /// Grab keyer parameters from the global variables
    pub(crate) unsafe fn from_globals() -> Result<Netkeyer, KeyerError> {
        let host = unsafe { CStr::from_ptr(&tlf::netkeyer_hostaddress as *const c_char) };
        let port = unsafe { tlf::netkeyer_port as c_uint }.try_into().unwrap();
        let netkeyer = Netkeyer::from_host_and_port(
            host.to_str().map_err(|_| KeyerError::InvalidDevice)?,
            port,
        )?;

        netkeyer.reset()?;
        netkeyer.set_weight(tlf::weight as i8)?;

        netkeyer.write_tone(get_tone())?;

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

        let sc_volume = Some(tlf::sc_volume).and_then(|v| v.try_into().ok());
        if let Some(sc_volume) = sc_volume {
            netkeyer.set_sidetone_volume(sc_volume)?;
        }

        Ok(netkeyer)
    }

    pub(crate) unsafe fn write_tone(&self, tone: i32) -> Result<(), KeyerError> {
        let tone = tone.try_into().map_err(|_| KeyerError::InvalidParameter)?;

        self.set_tone(tone)?;

        if tone != 0 {
            /* work around bugs in cwdaemon:
             * cwdaemon < 0.9.6 always set volume to 70% at change of tone freq
             * cwdaemon >=0.9.6 do not set volume at all after change of freq,
             * resulting in no tone output if you have a freq=0 in between
             * So... to be sure we set the volume back to our chosen value
             * or to 70% (like cwdaemon) if no volume got specified
             */
            let sc_volume: u8 = Some(tlf::sc_volume)
                .and_then(|v| v.try_into().ok())
                .unwrap_or(70);
            self.set_sidetone_volume(sc_volume)?;
        }
        Ok(())
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

        if !(5..=60).contains(&speed) {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "2{}", speed);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn set_tone(&self, tone: u16) -> Result<(), KeyerError> {
        let mut buf = make_buf::<6>();

        if tone != 0 && !(300..=1000).contains(&tone) {
            return Err(KeyerError::InvalidParameter);
        }

        write_esc!(buf, "3{}", tone);
        let _ = self.socket.send_to(extract_buf(&buf), self.dest_addr)?;
        Ok(())
    }

    pub(crate) fn abort(&self) -> Result<(), KeyerError> {
        self.simple_command(b'4')
    }

    #[allow(unused)]
    pub(crate) fn exit(&self) -> Result<(), KeyerError> {
        self.simple_command(b'5')
    }

    pub(crate) fn enable_word_mode(&self) -> Result<(), KeyerError> {
        self.simple_command(b'6')
    }

    pub(crate) fn set_weight(&self, weight: i8) -> Result<(), KeyerError> {
        let mut buf = make_buf::<6>();

        if !(-50..=50).contains(&weight) {
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

        if !(1..=9).contains(&bandindex) {
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

#[no_mangle]
pub unsafe extern "C" fn parse_tone(tonestr: *const c_char) -> c_int {
    CStr::from_ptr(tonestr)
        .to_str()
        .ok()
        .map(str::trim)
        .and_then(|t| t.parse::<c_int>().ok())
        .filter(|tone| *tone >= 0)
        .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn init_tone(tone: c_int) {
    TONE.store(tone, Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn get_tone() -> c_int {
    TONE.load(Ordering::SeqCst)
}

#[no_mangle]
pub unsafe extern "C" fn write_tone(tone: c_int) -> c_int {
    let prev_tone = TONE.swap(tone, Ordering::SeqCst);
    NETKEYER.with(|netkeyer| {
        if let Some(ref netkeyer) = *netkeyer.borrow() {
            netkeyer.write_tone(tone).expect("netkeyer send error");
        }
        // Ignore this call if netkeyer not initialized
    });

    prev_tone
}

#[no_mangle]
pub extern "C" fn netkeyer_set_ptt(ptt: bool) -> CResult {
    with_netkeyer(|netkeyer| netkeyer.set_ptt(ptt))
}

#[no_mangle]
pub extern "C" fn netkeyer_abort() -> CResult {
    with_netkeyer(|netkeyer| netkeyer.abort())
}

#[no_mangle]
pub extern "C" fn netkeyer_set_pin14(pin14: bool) -> CResult {
    with_netkeyer(|netkeyer| netkeyer.set_pin14(pin14))
}

#[no_mangle]
pub extern "C" fn netkeyer_reset() -> CResult {
    with_netkeyer(|netkeyer| netkeyer.reset())
}

#[no_mangle]
pub extern "C" fn netkeyer_tune(seconds: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        seconds
            .try_into()
            .ok()
            .and_then(|speed| netkeyer.tune(speed).ok())
    })
}

#[no_mangle]
pub extern "C" fn netkeyer_set_band_switch(bandidx: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        bandidx
            .try_into()
            .ok()
            .and_then(|bandidx| netkeyer.set_band_switch(bandidx).ok())
    })
}

#[no_mangle]
pub extern "C" fn netkeyer_enable_word_mode() -> CResult {
    with_netkeyer(|netkeyer| netkeyer.enable_word_mode())
}

#[no_mangle]
pub extern "C" fn netkeyer_set_sidetone_volume(volume: c_uint) -> CResult {
    with_netkeyer(|netkeyer| {
        volume
            .try_into()
            .ok()
            .and_then(|volume| netkeyer.set_sidetone_volume(volume).ok())
    })
}

fn with_netkeyer<R: Into<CResult>, F: FnOnce(&Netkeyer) -> R>(f: F) -> CResult {
    NETKEYER.with(|netkeyer| {
        if let Some(ref netkeyer) = *netkeyer.borrow() {
            f(netkeyer).into()
        } else {
            CResult::Err
        }
    })
}

pub struct NetKeyerFrontend(Arc<Netkeyer>);

impl NetKeyerFrontend {
    pub(crate) fn new(netkeyer: Arc<Netkeyer>) -> NetKeyerFrontend {
        NetKeyerFrontend(netkeyer)
    }
}

impl CwKeyerFrontend for NetKeyerFrontend {
    fn name(&self) -> &'static str {
        "cwdaemon"
    }

    fn set_speed(&mut self, speed: c_uint) -> Result<(), KeyerError> {
        let speed = speed.try_into().map_err(|_| KeyerError::InvalidParameter)?;
        self.0.set_speed(speed)
    }

    fn set_weight(&mut self, weight: c_int) -> Result<(), KeyerError> {
        let weight = weight
            .try_into()
            .map_err(|_| KeyerError::InvalidParameter)?;
        self.0.set_weight(weight)
    }

    fn stop_keying(&mut self) -> Result<(), KeyerError> {
        self.0.abort()
    }

    fn reset(&mut self) -> Result<(), KeyerError> {
        self.0.reset()
    }
}

impl CwKeyerBackend for Arc<Netkeyer> {
    fn send_message(&mut self, msg: Vec<u8>) -> Result<(), KeyerError> {
        self.send_text(&msg)
    }
}
