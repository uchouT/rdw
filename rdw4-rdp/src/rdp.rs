use std::sync::{Arc, Mutex};

use ironrdp::{
    cliprdr::{self, backend::ClipboardMessage, pdu::ClipboardFormat},
    connector::{
        connection_activation::ConnectionActivationState, ClientConnector, Config,
        ConnectionResult, DesktopSize,
    },
    displaycontrol::{client::DisplayControlClient, pdu::MonitorLayoutEntry},
    graphics::{image_processing::PixelFormat as IronRdpPixelFormat, pointer::DecodedPointer},
    pdu::{
        geometry::InclusiveRectangle,
        input::{
            fast_path::{FastPathInputEvent, KeyboardFlags},
            mouse::PointerFlags,
            MousePdu,
        },
        WriteBuf,
    },
    rdpdr::{self, NoopRdpdrBackend},
    rdpsnd,
    session::{
        fast_path, image::DecodedImage, ActiveStage, ActiveStageOutput, GracefulDisconnectReason,
    },
};
use ironrdp_tokio::{
    reqwest::ReqwestNetworkClient, single_sequence_step_read, split_tokio_framed, FramedWrite,
};
use rdw::{gtk::gdk, KeyEvent};
use tokio::{net::TcpStream, sync::mpsc};
use tracing::{debug, trace, warn};

use crate::{
    cliprdr::{CbBackend, PasteProgress},
    snd::RdwSndBackend,
    Error, Result,
};

enum RdpControlFlow {
    ReconnectWithNewSize { width: u16, height: u16 },
    TerminatedGracefully(GracefulDisconnectReason),
}

type UpgradedFramed = ironrdp_tokio::TokioFramed<ironrdp_tls::TlsStream<TcpStream>>;

pub(crate) async fn rdp_run(
    domain: &str,
    port: u16,
    mut config: Config,
    input_event_rx: &mut mpsc::UnboundedReceiver<RdpInputEvent>,
    output_event_tx: &mpsc::UnboundedSender<RdpOutputEvent>,
    rdp_paste: PasteProgress,
) {
    loop {
        let cb_backend = CbBackend::new(output_event_tx.clone(), rdp_paste.clone());
        let (connection_result, framed) =
            match tls_connect(domain, port, config.clone(), cb_backend).await {
                Ok(res) => res,
                Err(err) => {
                    let _ = output_event_tx.send(RdpOutputEvent::Terminated(Err(err)));
                    break;
                }
            };

        let _ = output_event_tx.send(RdpOutputEvent::Connected);

        match active_session(framed, connection_result, input_event_rx, output_event_tx).await {
            Ok(RdpControlFlow::ReconnectWithNewSize { width, height }) => {
                config.desktop_size = DesktopSize { width, height };
            }
            Ok(RdpControlFlow::TerminatedGracefully(reason)) => {
                debug!(?reason, "RDP session terminated gracefully");
                let _ = output_event_tx.send(RdpOutputEvent::Terminated(Ok(reason)));
                break;
            }
            Err(err) => {
                debug!(?err, "RDP session error");
                let _ = output_event_tx.send(RdpOutputEvent::Terminated(Err(err)));
                break;
            }
        }
    }
}

async fn tls_connect(
    domain: &str,
    port: u16,
    config: Config,
    cb_backend: CbBackend,
) -> Result<(ConnectionResult, UpgradedFramed)> {
    let stream = TcpStream::connect(format!("{domain}:{port}")).await?;
    let server_addr = stream.peer_addr()?;

    let mut framed = ironrdp_tokio::TokioFramed::new(stream);
    let snd_backend = RdwSndBackend::new()?;

    let mut connector = ClientConnector::new(config, server_addr)
        .with_static_channel(
            ironrdp::dvc::DrdynvcClient::new()
                .with_dynamic_channel(DisplayControlClient::new(|_| Ok(Vec::new()))),
        )
        .with_static_channel(
            rdpdr::Rdpdr::new(Box::new(NoopRdpdrBackend {}), "IronRDP".to_owned())
                .with_smartcard(0),
        )
        .with_static_channel(rdpsnd::client::Rdpsnd::new(Box::new(snd_backend)))
        .with_static_channel(cliprdr::Cliprdr::new(Box::new(cb_backend)));

    let should_upgrade = ironrdp_tokio::connect_begin(&mut framed, &mut connector).await?;

    debug!("TLS upgrade");

    // Ensure there is no leftover
    let initial_stream = framed.into_inner_no_leftover();

    let (upgraded_stream, server_public_key) = ironrdp_tls::upgrade(initial_stream, domain).await?;

    let upgraded = ironrdp_tokio::mark_as_upgraded(should_upgrade, &mut connector);

    let mut upgraded_framed = ironrdp_tokio::TokioFramed::new(upgraded_stream);

    let mut network_client = ReqwestNetworkClient::new();
    let connection_result = ironrdp_tokio::connect_finalize(
        upgraded,
        &mut upgraded_framed,
        connector,
        domain.into(),
        server_public_key,
        Some(&mut network_client),
        None,
    )
    .await?;

    debug!(?connection_result);

    Ok((connection_result, upgraded_framed))
}

async fn active_session(
    framed: UpgradedFramed,
    connection_result: ConnectionResult,
    input_event_rx: &mut mpsc::UnboundedReceiver<RdpInputEvent>,
    output_event_tx: &mpsc::UnboundedSender<RdpOutputEvent>,
) -> Result<RdpControlFlow> {
    let (mut reader, mut writer) = split_tokio_framed(framed);
    let mut image_arc = Arc::new(Mutex::new(DecodedImage::new(
        IronRdpPixelFormat::RgbA32,
        connection_result.desktop_size.width,
        connection_result.desktop_size.height,
    )));

    let mut active_stage = ActiveStage::new(connection_result);

    let disconnect_reason = 'outer: loop {
        let outputs = tokio::select! {
            frame = reader.read_pdu() => {
                let (action, payload) = frame?;
                trace!(?action, frame_length = payload.len(), "Frame received");

                let mut image = image_arc.lock().unwrap();
                active_stage.process(&mut image, action, &payload)?
            }
            input_event = input_event_rx.recv() => {
                let input_event = input_event.ok_or_else(|| { Error::Other("Input event not received".to_string()) })?;

                match input_event {
                    RdpInputEvent::Resize { width, height, scale_factor, physical_size } => {
                        trace!(width, height, "Resize event");
                        let (width, height) = MonitorLayoutEntry::adjust_display_size(width.into(), height.into());
                        debug!(width, height, "Adjusted display size");
                        if let Some(response_frame) = active_stage.encode_resize(width, height, Some(scale_factor), physical_size) {
                            vec![ActiveStageOutput::ResponseFrame(response_frame?)]
                        } else {
                            // TODO(#271): use the "auto-reconnect cookie": https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/15b0d1c9-2891-4adb-a45e-deb4aeeeab7c
                            debug!("Reconnecting with new size");
                            return Ok(RdpControlFlow::ReconnectWithNewSize { width: width.try_into().unwrap(), height: height.try_into().unwrap() })
                        }
                    },
                    RdpInputEvent::FastPath(events) => {
                        trace!(?events);
                        let mut image = image_arc.lock().unwrap();
                        active_stage.process_fastpath_input(&mut image, &events)?
                    }
                    RdpInputEvent::Close => {
                        active_stage.graceful_shutdown()?
                    }
                    RdpInputEvent::ClipboardResponseInitialCopy(formats) => {
                        match cliprdr_handle_message(&mut active_stage, ClipboardMessage::SendInitiateCopy(formats), true) {
                            Ok(resp) => resp,
                            Err(err) => {
                                warn!("Error handling clipboard message: {}", err);
                                continue 'outer;
                            }
                        }
                    }
                    RdpInputEvent::Clipboard(message) => {
                        debug!(?message);
                        match cliprdr_handle_message(&mut active_stage, message, false) {
                            Ok(resp) => resp,
                            Err(err) => {
                                warn!("Error handling clipboard message: {}", err);
                                continue 'outer;
                            }
                        }

                    }
                }
            }
        };

        for out in outputs {
            match out {
                ActiveStageOutput::ResponseFrame(frame) => writer.write_all(&frame).await?,
                ActiveStageOutput::GraphicsUpdate(region) => {
                    let image = image_arc.clone();
                    let _ = output_event_tx.send(RdpOutputEvent::GraphicsUpdate { image, region });
                }
                ActiveStageOutput::PointerDefault => {
                    let _ = output_event_tx.send(RdpOutputEvent::PointerDefault);
                }
                ActiveStageOutput::PointerHidden => {
                    let _ = output_event_tx.send(RdpOutputEvent::PointerHidden);
                }
                ActiveStageOutput::PointerPosition { x, y } => {
                    let _ = output_event_tx.send(RdpOutputEvent::PointerPosition { x, y });
                }
                ActiveStageOutput::PointerBitmap(pointer) => {
                    let _ = output_event_tx.send(RdpOutputEvent::PointerBitmap(pointer));
                }
                ActiveStageOutput::DeactivateAll(mut connection_activation) => {
                    // Execute the Deactivation-Reactivation Sequence:
                    // https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/dfc234ce-481a-4674-9a5d-2a7bafb14432
                    debug!("Received Server Deactivate All PDU, executing Deactivation-Reactivation Sequence");
                    let mut buf = WriteBuf::new();
                    'activation_seq: loop {
                        let written = single_sequence_step_read(
                            &mut reader,
                            &mut *connection_activation,
                            &mut buf,
                        )
                        .await?;

                        if written.size().is_some() {
                            writer.write_all(buf.filled()).await?;
                        }

                        if let ConnectionActivationState::Finalized {
                            io_channel_id,
                            user_channel_id,
                            desktop_size,
                            no_server_pointer,
                            pointer_software_rendering,
                        } = connection_activation.state
                        {
                            debug!(
                                ?desktop_size,
                                "Deactivation-Reactivation Sequence completed"
                            );
                            // Update image size with the new desktop size.
                            image_arc = Arc::new(Mutex::new(DecodedImage::new(
                                IronRdpPixelFormat::RgbA32,
                                desktop_size.width,
                                desktop_size.height,
                            )));
                            // Update the active stage with the new channel IDs and pointer settings.
                            active_stage.set_fastpath_processor(
                                fast_path::ProcessorBuilder {
                                    io_channel_id,
                                    user_channel_id,
                                    no_server_pointer,
                                    pointer_software_rendering,
                                }
                                .build(),
                            );
                            active_stage.set_no_server_pointer(no_server_pointer);
                            break 'activation_seq;
                        }
                    }
                }
                ActiveStageOutput::Terminate(reason) => break 'outer reason,
            }
        }
    };

    Ok(RdpControlFlow::TerminatedGracefully(disconnect_reason))
}

fn cliprdr_handle_message(
    active_stage: &mut ActiveStage,
    message: ClipboardMessage,
    is_initiate: bool,
) -> Result<Vec<ActiveStageOutput>> {
    let Some(cliprdr) = active_stage.get_svc_processor::<cliprdr::CliprdrClient>() else {
        return Err(Error::Other("No clipboard processor".to_owned()));
    };
    let be: &CbBackend = cliprdr.downcast_backend().unwrap();
    if !is_initiate && !be.ready {
        return Err(Error::Other("Clipboard processor not ready".to_owned()));
    }
    let svc_messages = match message {
        ClipboardMessage::SendInitiateCopy(formats) => cliprdr.initiate_copy(&formats)?,
        ClipboardMessage::SendFormatData(response) => cliprdr.submit_format_data(response)?,
        ClipboardMessage::SendInitiatePaste(format) => cliprdr.initiate_paste(format)?,
        ClipboardMessage::Error(e) => {
            return Err(Error::Other(format!("Clipboard backend error: {e}")));
        }
    };
    let frame = active_stage.process_svc_processor_messages(svc_messages)?;

    Ok(vec![ActiveStageOutput::ResponseFrame(frame)])
}

#[derive(Debug)]
pub enum RdpInputEvent {
    Resize {
        width: u16,
        height: u16,
        scale_factor: u32,
        /// The physical size of the display in millimeters (width, height).
        physical_size: Option<(u32, u32)>,
    },
    FastPath(Vec<FastPathInputEvent>),
    Close,
    ClipboardResponseInitialCopy(Vec<ClipboardFormat>),
    Clipboard(ClipboardMessage),
}

impl RdpInputEvent {
    pub(crate) fn create_channel() -> (mpsc::UnboundedSender<Self>, mpsc::UnboundedReceiver<Self>) {
        mpsc::unbounded_channel()
    }
}

#[derive(Debug)]
pub enum RdpOutputEvent {
    Connected,
    GraphicsUpdate {
        image: Arc<Mutex<DecodedImage>>,
        region: InclusiveRectangle,
    },
    PointerDefault,
    PointerHidden,
    PointerBitmap(Arc<DecodedPointer>),
    PointerPosition {
        x: u16,
        y: u16,
    },
    ClipboardRequestFormatList,
    Clipboard(ClipboardMessage),
    Terminated(Result<GracefulDisconnectReason>),
}

impl RdpOutputEvent {
    pub(crate) fn create_channel() -> (mpsc::UnboundedSender<Self>, mpsc::UnboundedReceiver<Self>) {
        mpsc::unbounded_channel()
    }
}

pub(crate) fn rdp_motion(x_position: u16, y_position: u16) -> Vec<FastPathInputEvent> {
    vec![FastPathInputEvent::MouseEvent(MousePdu {
        flags: PointerFlags::MOVE,
        number_of_wheel_rotation_units: 0,
        x_position,
        y_position,
    })]
}

fn gdk_button_to_pointer_flags(button: u32) -> Option<PointerFlags> {
    let flags = match button {
        gdk::BUTTON_PRIMARY => PointerFlags::LEFT_BUTTON,
        gdk::BUTTON_MIDDLE => PointerFlags::MIDDLE_BUTTON_OR_WHEEL,
        gdk::BUTTON_SECONDARY => PointerFlags::RIGHT_BUTTON,
        _ => {
            debug!(?button, "Unhandled button");
            return None;
        }
    };

    Some(flags)
}

pub(crate) fn rdp_mouse_press(
    x_position: u16,
    y_position: u16,
    button: u32,
) -> Vec<FastPathInputEvent> {
    let Some(flags) = gdk_button_to_pointer_flags(button) else {
        return vec![];
    };
    vec![FastPathInputEvent::MouseEvent(MousePdu {
        flags: PointerFlags::DOWN | flags,
        number_of_wheel_rotation_units: 0,
        x_position,
        y_position,
    })]
}

pub(crate) fn rdp_mouse_release(
    x_position: u16,
    y_position: u16,
    button: u32,
) -> Vec<FastPathInputEvent> {
    let Some(flags) = gdk_button_to_pointer_flags(button) else {
        return vec![];
    };
    vec![FastPathInputEvent::MouseEvent(MousePdu {
        flags,
        number_of_wheel_rotation_units: 0,
        x_position,
        y_position,
    })]
}

pub(crate) fn rdp_key_event(xt: u16, event: KeyEvent) -> Vec<FastPathInputEvent> {
    let mut fp = vec![];
    let flags = if xt & 0x100 > 0 {
        // to check (ironrdp checks 0xE000, but might be different scancode)
        KeyboardFlags::EXTENDED
    } else {
        KeyboardFlags::empty()
    };

    if event.contains(rdw::KeyEvent::PRESS) {
        fp.push(FastPathInputEvent::KeyboardEvent(flags, xt as _));
    }
    if event.contains(rdw::KeyEvent::RELEASE) {
        fp.push(FastPathInputEvent::KeyboardEvent(
            flags | KeyboardFlags::RELEASE,
            xt as _,
        ));
    }

    fp
}

pub(crate) fn rdp_scroll(
    dx: i16,
    dy: i16,
    x_position: u16,
    y_position: u16,
) -> Vec<FastPathInputEvent> {
    let mut fp = vec![];
    if dx != 0 {
        fp.push(FastPathInputEvent::MouseEvent(MousePdu {
            flags: PointerFlags::HORIZONTAL_WHEEL,
            number_of_wheel_rotation_units: -dx,
            x_position,
            y_position,
        }));
    }
    if dy != 0 {
        fp.push(FastPathInputEvent::MouseEvent(MousePdu {
            flags: PointerFlags::VERTICAL_WHEEL,
            number_of_wheel_rotation_units: dy,
            x_position,
            y_position,
        }));
    }
    fp
}
