use super::{
    handshake::{self, Handshake},
    protocol::{decode_event, encode_event, ChatEvent, KIND_CHUNK, KIND_EVENT, KIND_READY},
    state::{lock, Phase, Session, Verification},
};
use crate::errors::AppError;
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use webrtc::data_channel::RTCDataChannel;

pub(super) const BUFFER_HIGH: usize = 256 * 1024;
pub(super) const BUFFER_LOW: usize = 64 * 1024;
pub(super) const MAX_INBOX_EVENTS: usize = 512;
pub(super) const MAX_INBOX_BYTES: usize = 8 * 1024 * 1024;
// A rate-limited TURN route may drain SCTP for longer than five seconds.
// Session stop still cancels either wait immediately.
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

async fn sending_guard(s: &Session) -> Result<tokio::sync::MutexGuard<'_, ()>, AppError> {
    tokio::select! { biased;
        _ = s.stopped() => Err(AppError::new("session_closed", "session", "Сессия завершена", true)),
        guard = s.sending.lock() => { s.ensure_active()?; Ok(guard) },
    }
}

pub fn attach_dc(s: &Arc<Session>, dc: &Arc<RTCDataChannel>) -> Result<(), AppError> {
    s.ensure_active()?;
    if dc.label() != "ssc-v4"
        || !dc.ordered()
        || dc.max_retransmits().is_some()
        || dc.max_packet_lifetime().is_some()
    {
        return Err(AppError::new(
            "invalid_channel",
            "data_channel",
            "Недопустимый канал соединения",
            false,
        ));
    }
    {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        if d.dc.is_some() {
            return Err(AppError::new(
                "duplicate_channel",
                "key_exchange",
                "Повторный канал отклонён",
                false,
            ));
        }
        let role = d.role.ok_or_else(|| {
            AppError::new(
                "role_required",
                "key_exchange",
                "Не определена роль подключения",
                false,
            )
        })?;
        d.exchange = Some(Handshake::new(&d.connection_id, role)?);
        d.dc = Some(dc.clone());
    }
    let weak = Arc::downgrade(s);
    dc.on_open(Box::new(move || {
        let weak = weak.clone();
        Box::pin(async move {
            if let Some(s) = weak.upgrade() {
                {
                    let mut d = lock(&s.data);
                    if d.closed {
                        return;
                    }
                    d.dc_open = true;
                }
                let dc = lock(&s.data).dc.clone();
                if let Some(dc) = dc {
                    dc.set_buffered_amount_low_threshold(BUFFER_LOW).await;
                }
                s.start_handshake_timeout();
                if let Err(error) = start_handshake(&s).await {
                    s.error(&error);
                    s.close_from_callback();
                } else {
                    s.publish();
                }
            }
        })
    }));
    let weak = Arc::downgrade(s);
    dc.on_message(Box::new(move |msg| {
        let weak = weak.clone();
        Box::pin(async move {
            if let Some(s) = weak.upgrade() {
                if !s.active() {
                    return;
                }
                let result = if handshake::is_handshake(&msg.data) {
                    receive_handshake(&s, &msg.data).await
                } else {
                    receive(&s, &msg.data)
                };
                if let Err(error) = result {
                    s.error(&error);
                    s.close_from_callback();
                }
            }
        })
    }));
    let weak = Arc::downgrade(s);
    dc.on_close(Box::new(move || {
        let weak = weak.clone();
        Box::pin(async move {
            if let Some(s) = weak.upgrade() {
                if s.active() {
                    s.close_from_callback();
                }
            }
        })
    }));
    Ok(())
}

async fn send_handshake(s: &Arc<Session>, frames: Vec<Vec<u8>>) -> Result<(), AppError> {
    for frame in frames {
        s.ensure_active()?;
        let dc = lock(&s.data).dc.clone().ok_or_else(|| {
            AppError::new("channel_closed", "key_exchange", "Канал недоступен", true)
        })?;
        let frame = Bytes::from(frame);
        tokio::select! { biased;
            _ = s.stopped() => return s.ensure_active(),
            result = dc.send(&frame) => { result.map_err(|_| AppError::new("key_send", "key_exchange", "Не удалось передать обмен ключами", true))?; },
        }
    }
    s.ensure_active()
}

async fn start_handshake(s: &Arc<Session>) -> Result<(), AppError> {
    let _sending = sending_guard(s).await?;
    s.ensure_active()?;
    let frames = lock(&s.data)
        .exchange
        .as_mut()
        .ok_or_else(|| {
            AppError::new(
                "key_required",
                "key_exchange",
                "Нет состояния обмена ключами",
                false,
            )
        })?
        .start()?;
    send_handshake(s, frames).await
}

pub(super) async fn receive_handshake(s: &Arc<Session>, bytes: &[u8]) -> Result<(), AppError> {
    // A peer commitment can arrive concurrently with on_open. This guard keeps our
    // commitment on the reliable ordered channel ahead of our reveal.
    let _sending = sending_guard(s).await?;
    s.ensure_active()?;
    let (frames, verification) = {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        let step = d
            .exchange
            .as_mut()
            .ok_or_else(|| {
                AppError::new(
                    "duplicate_handshake",
                    "key_exchange",
                    "Повторный обмен ключами",
                    false,
                )
            })?
            .receive(bytes)?;
        let verification = if let Some(crypto) = step.crypto {
            let verification = Verification {
                generation: s.generation,
                sas: crypto.sas.clone(),
            };
            d.crypto = Some(crypto);
            d.exchange = None;
            Some(verification)
        } else {
            None
        };
        (step.outgoing, verification)
    };
    send_handshake(s, frames).await?;
    if let Some(verification) = verification {
        s.emit("ssc-sas", verification);
    }
    s.publish();
    Ok(())
}

pub(super) fn receive(s: &Arc<Session>, bytes: &[u8]) -> Result<(), AppError> {
    let event = {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        let crypto = d.crypto.as_mut().ok_or_else(|| {
            AppError::new(
                "key_required",
                "key_exchange",
                "Обмен ключами не завершён",
                false,
            )
        })?;
        let (kind, payload) = crypto.decrypt(bytes)?;
        match kind {
            KIND_READY => {
                if !payload.is_empty() {
                    return Err(AppError::new(
                        "ready_payload",
                        "verification",
                        "Некорректное подтверждение",
                        false,
                    ));
                }
                d.peer_ready = true;
                None
            }
            KIND_CHUNK => {
                if !d.local_ready || !d.peer_ready {
                    return Err(AppError::new(
                        "not_ready",
                        "verification",
                        "Сверка кода ещё не завершена",
                        false,
                    ));
                }
                d.reassembler
                    .push_with_id(&payload)?
                    .map(|message| ChatEvent::Text {
                        id: message.id,
                        text: message.text,
                    })
            }
            KIND_EVENT => {
                if !d.local_ready || !d.peer_ready {
                    return Err(AppError::new(
                        "not_ready",
                        "verification",
                        "Сверка кода ещё не завершена",
                        false,
                    ));
                }
                Some(decode_event(&payload)?)
            }
            _ => {
                return Err(AppError::new(
                    "frame_type",
                    "crypto",
                    "Неизвестный тип сообщения",
                    false,
                ))
            }
        }
    };
    if let Some(event) = event {
        if let ChatEvent::Bye { reason } = event {
            s.close_authenticated(reason);
            return Ok(());
        }
        queue_received(s, event)?;
    }
    s.publish();
    Ok(())
}

pub(super) fn queue_received(s: &Arc<Session>, event: ChatEvent) -> Result<(), AppError> {
    event.validate()?;
    let size = serde_json::to_vec(&event)
        .map_err(|_| {
            AppError::new(
                "invalid_event",
                "message",
                "Недопустимое событие чата",
                false,
            )
        })?
        .len();
    let text = match &event {
        ChatEvent::Text { id, text } => Some((id.clone(), text.clone())),
        _ => None,
    };
    {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        if d.event_inbox.len() >= MAX_INBOX_EVENTS
            || size > MAX_INBOX_BYTES.saturating_sub(d.event_bytes)
        {
            drop(d);
            s.close_from_callback();
            return Err(AppError::new(
                "inbox_full",
                "message",
                "Переполнен буфер входящих сообщений",
                true,
            ));
        }
        if let Some((id, text)) = &text {
            d.inbox.push(super::state::ReceivedMessage {
                generation: s.generation,
                id: id.clone(),
                text: text.clone(),
            });
        }
        d.event_bytes += size;
        d.event_inbox
            .push(super::state::QueuedEvent { event, bytes: size });
    }
    if let Some((id, text)) = text {
        s.emit(
            "ssc-chat-message",
            serde_json::json!({"generation":s.generation,"id":id,"text":text}),
        );
        crate::diagnostics::record(s.generation, "chat", "message", "received", None);
    }
    Ok(())
}

async fn wait_capacity(
    s: &Arc<Session>,
    dc: &RTCDataChannel,
    frame_bytes: usize,
) -> Result<(), AppError> {
    wait_send_buffer(s, frame_bytes, || dc.buffered_amount()).await
}

pub(super) async fn wait_send_buffer<F, Fut>(
    s: &Arc<Session>,
    frame_bytes: usize,
    amount: F,
) -> Result<(), AppError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = usize>,
{
    let waiting = async {
        if amount().await.saturating_add(frame_bytes) > BUFFER_HIGH {
            while amount().await > BUFFER_LOW {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    };
    tokio::select! { biased;
        _ = s.stopped() => s.ensure_active(),
        result = tokio::time::timeout(SEND_TIMEOUT, waiting) => {
            result.map_err(|_|AppError::new("backpressure_timeout", "data_channel", "Канал не успевает передавать данные", true))?;
            s.ensure_active()
        }
    }
}
pub async fn send_encrypted(s: &Arc<Session>, kind: u8, payload: &[u8]) -> Result<(), AppError> {
    s.ensure_active()?;
    let dc = lock(&s.data)
        .dc
        .clone()
        .ok_or_else(|| AppError::new("channel_closed", "data_channel", "Канал недоступен", true))?;
    wait_capacity(s, &dc, payload.len().saturating_add(26)).await?;
    let (dc, frame) = {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        if kind != KIND_READY && (!d.local_ready || !d.peer_ready) {
            return Err(AppError::new(
                "not_ready",
                "verification",
                "Сверка кода ещё не завершена",
                true,
            ));
        }
        let dc = d.dc.clone().filter(|_| d.dc_open).ok_or_else(|| {
            AppError::new("channel_closed", "data_channel", "Канал недоступен", true)
        })?;
        let crypto = d
            .crypto
            .as_mut()
            .ok_or_else(|| AppError::new("key_required", "crypto", "Нет ключей сессии", false))?;
        (dc, crypto.encrypt(kind, payload)?)
    };
    let frame = Bytes::from(frame);
    tokio::select! { biased;
        _ = s.stopped() => return s.ensure_active(),
        result = tokio::time::timeout(SEND_TIMEOUT, dc.send(&frame)) => {
            result.map_err(|_| AppError::new("send_failed", "data_channel", "Не удалось передать данные каналу", true))?
                .map_err(|_| AppError::new("send_failed", "message", "Не удалось передать сообщение каналу", true))?;
        },
    };
    s.ensure_active()
}
pub async fn confirm(
    s: &Arc<Session>,
    generation: u64,
    expected_sas: &str,
) -> Result<bool, AppError> {
    let _sending = sending_guard(s).await?;
    s.ensure_active()?;
    {
        let mut d = lock(&s.data);
        if d.closed {
            return Err(AppError::new(
                "session_closed",
                "session",
                "Сессия завершена",
                true,
            ));
        }
        if s.generation != generation
            || d.crypto
                .as_ref()
                .is_none_or(|crypto| crypto.sas != expected_sas)
        {
            return Err(AppError::new(
                "verification_stale",
                "verification",
                "Код или подключение изменились; повторите сверку",
                true,
            ));
        }
        if d.local_ready {
            return Ok(true);
        }
        d.local_ready = true;
    }
    if let Err(error) = send_encrypted(s, KIND_READY, &[]).await {
        lock(&s.data).local_ready = false;
        return Err(error);
    }
    s.publish();
    Ok(true)
}
pub async fn send_text_session(s: &Arc<Session>, text: &str) -> Result<bool, AppError> {
    let parts = super::protocol::chunks(text)?;
    let _sending = sending_guard(s).await?;
    if s.snapshot().phase != Phase::Connected {
        return Err(AppError::new(
            "not_ready",
            "verification",
            "Ожидаем подтверждения собеседника",
            true,
        ));
    }
    for payload in parts {
        send_encrypted(s, KIND_CHUNK, &payload).await?;
    }
    crate::diagnostics::record(s.generation, "chat", "message", "sent_to_channel", None);
    Ok(true)
}

pub async fn send_chat_event_session(
    s: &Arc<Session>,
    generation: u64,
    event: ChatEvent,
) -> Result<(), AppError> {
    s.ensure_generation_ready(generation)?;
    event.validate()?;
    if let ChatEvent::Bye { reason } = event {
        s.close_local(reason).await;
        return Ok(());
    }
    let _sending = sending_guard(s).await?;
    s.ensure_generation_ready(generation)?;
    if s.snapshot().phase != Phase::Connected {
        return Err(AppError::new(
            "not_ready",
            "verification",
            "Ожидаем соединения с собеседником",
            true,
        ));
    }
    if let ChatEvent::Text { id, text } = event {
        let mut message_id = [0u8; 16];
        hex::decode_to_slice(id, &mut message_id).map_err(|_| {
            AppError::new(
                "invalid_event",
                "message",
                "Недопустимое событие чата",
                false,
            )
        })?;
        for part in ssc_protocol::message::chunks_with_id(&text, message_id)? {
            send_encrypted(s, KIND_CHUNK, &part).await?;
        }
    } else {
        let payload = encode_event(&event)?;
        send_encrypted(s, KIND_EVENT, &payload).await?;
    }
    crate::diagnostics::record(s.generation, "chat", "message", "sent_to_channel", None);
    Ok(())
}
