use std::sync::{Arc, Mutex};

use ironrdp::{
    cliprdr::{
        backend::{ClipboardMessage, ClipboardMessageProxy, CliprdrBackend},
        pdu::{
            ClipboardFormat, ClipboardFormatId, ClipboardFormatName,
            ClipboardGeneralCapabilityFlags, FileContentsRequest, FileContentsResponse,
            FormatDataRequest, FormatDataResponse, LockDataId,
        },
    },
    core::impl_as_any,
};
use rdw::gtk::gdk;
use tokio::sync::{mpsc, oneshot::Sender};
use tracing::{debug, error, trace, warn};

use crate::{rdp::RdpOutputEvent, util::string_from_utf16};

#[derive(Clone, Debug)]
pub(crate) struct ClientClipboardMessageProxy {
    tx: mpsc::UnboundedSender<RdpOutputEvent>,
}

impl ClientClipboardMessageProxy {
    pub fn new(tx: mpsc::UnboundedSender<RdpOutputEvent>) -> Self {
        Self { tx }
    }
}

impl ClipboardMessageProxy for ClientClipboardMessageProxy {
    fn send_clipboard_message(&self, message: ClipboardMessage) {
        if self.tx.send(RdpOutputEvent::Clipboard(message)).is_err() {
            error!("Failed to send clipboard message, receiver is closed");
        }
    }
}

type InnerPaste = Option<(ClipboardFormatId, Sender<Vec<u8>>)>;

#[derive(Debug, Clone)]
pub(crate) struct PasteProgress {
    inner: Arc<Mutex<InnerPaste>>,
}

impl PasteProgress {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn set(&self, format_id: ClipboardFormatId, sender: Sender<Vec<u8>>) {
        *self.inner.lock().unwrap() = Some((format_id, sender));
    }

    pub(crate) fn take(&self) -> Option<(ClipboardFormatId, Sender<Vec<u8>>)> {
        self.inner.lock().unwrap().take()
    }

    pub(crate) fn is_some(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }
}

#[derive(Debug)]
pub(crate) struct CbBackend {
    proxy: ClientClipboardMessageProxy,
    paste: PasteProgress,
    pub(crate) ready: bool,
}

impl_as_any!(CbBackend);

impl CbBackend {
    pub fn new(tx: mpsc::UnboundedSender<RdpOutputEvent>, paste: PasteProgress) -> Self {
        Self {
            proxy: ClientClipboardMessageProxy::new(tx),
            paste,
            ready: false,
        }
    }
}

impl CliprdrBackend for CbBackend {
    fn temporary_directory(&self) -> &str {
        ".cliprdr"
    }

    fn on_ready(&mut self) {
        self.ready = true
    }

    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        // No additional capabilities yet
        ClipboardGeneralCapabilityFlags::empty()
    }

    fn on_process_negotiated_capabilities(
        &mut self,
        capabilities: ClipboardGeneralCapabilityFlags,
    ) {
        trace!(?capabilities);
    }

    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        trace!(?available_formats);
        self.proxy
            .send_clipboard_message(ClipboardMessage::SendInitiateCopy(
                available_formats.to_vec(),
            ))
    }

    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        trace!(?request);
        self.proxy
            .send_clipboard_message(ClipboardMessage::SendInitiatePaste(request.format))
    }

    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        trace!(?response);
        let Some((format, tx)) = self.paste.take() else {
            warn!("Got format data response without a pending paste request");
            return;
        };

        let data = match format {
            ClipboardFormatId::CF_UNICODETEXT => match string_from_utf16(response.data()) {
                Ok(res) => res.into_bytes(),
                Err(e) => {
                    warn!("Invalid utf16 clipboard text: {e}");
                    return;
                }
            },
            _ => response.data().to_vec(),
        };
        let _ = tx.send(data);
    }

    fn on_file_contents_request(&mut self, request: FileContentsRequest) {
        debug!(?request);
    }

    fn on_file_contents_response(&mut self, response: FileContentsResponse<'_>) {
        debug!(?response);
    }

    fn on_lock(&mut self, data_id: LockDataId) {
        debug!(?data_id);
    }

    fn on_unlock(&mut self, data_id: LockDataId) {
        debug!(?data_id);
    }

    fn on_request_format_list(&mut self) {
        trace!("on_request_format_list");
        if self
            .proxy
            .tx
            .send(RdpOutputEvent::ClipboardRequestFormatList)
            .is_err()
        {
            error!("Failed to send clipboard message, receiver is closed");
        }
    }
}

pub(crate) fn rdp_clipboard_formats(formats: gdk::ContentFormats) -> Vec<ClipboardFormat> {
    let mut formats = formats
        .mime_types()
        .iter()
        .map(|m| rdp_clipboard_format(m.as_str()))
        .collect::<Vec<_>>();
    formats.dedup();
    formats
}

pub(crate) fn rdp_clipboard_format(mime: &str) -> ClipboardFormat {
    match mime {
        "text/plain" | "text/plain;charset=utf-8" | "UTF8_STRING" | "TEXT" | "STRING" => {
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)
        }
        // "image/bmp" => Some(Format::Dib),
        // "text/html" => Some(Format::Html),
        // "image/png" => Some(Format::Png),
        // "image/jpeg" => Some(Format::Jpeg),
        // "image/gif" => Some(Format::Gif),
        // "text/uri-list" => Some(Format::TextUriList),
        other => ClipboardFormat::new(ClipboardFormatId(0))
            .with_name(ClipboardFormatName::new(other.to_string())),
    }
}

pub(crate) fn rdp_clipboard_mime_from_id(id: ClipboardFormatId) -> Option<&'static str> {
    match id {
        ClipboardFormatId::CF_TEXT
        | ClipboardFormatId::CF_OEMTEXT
        | ClipboardFormatId::CF_UNICODETEXT => Some("text/plain;charset=utf-8"),
        ClipboardFormatId::CF_DIB | ClipboardFormatId::CF_DIBV5 => Some("image/bmp"),
        // ClipboardFormatId::CF_
        _ => None,
    }
}

pub(crate) fn rdp_clipboard_mime(format: &ClipboardFormat) -> Option<&'static str> {
    rdp_clipboard_mime_from_id(format.id())
}
