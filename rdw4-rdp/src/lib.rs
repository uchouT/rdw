use ironrdp::{connector::ConnectorError, pdu::PduError, session::SessionError};
use thiserror::Error;

pub use ironrdp;
pub use rdw;

mod display;
pub use display::*;

mod cliprdr;
mod rdp;
mod snd;
mod util;

#[cfg(feature = "capi")]
mod capi;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Already connected")]
    AlreadyConnected,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Pdu error: {0}")]
    Pdu(#[from] PduError),

    #[error("Connector error: {0}")]
    Connector(#[from] ConnectorError),

    #[error("Session error: {0}")]
    Session(#[from] SessionError),

    #[error("Other error: {0}")]
    Other(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
