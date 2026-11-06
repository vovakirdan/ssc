use super::{
    handshake::{self, Handshake},
    protocol::{KIND_CHUNK, KIND_READY},
    state::{lock, Phase, Session, Verification},
};
use crate::errors::AppError;
use bytes::Bytes;
use std::sync::Arc;
use webrtc::data_channel::RTCDataChannel;

pub fn attach_dc(s: &Arc<Session>, dc: &Arc<RTCDataChannel>) -> Result<(), AppError> {
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
        dc.send(&Bytes::from(frame)).await.map_err(|_| {
            AppError::new(
                "key_send",
                "key_exchange",
                "Не удалось передать обмен ключами",
                true,
            )
        })?;
    }
    s.ensure_active()
}

async fn start_handshake(s: &Arc<Session>) -> Result<(), AppError> {
    let _sending = s.sending.lock().await;
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
    let _sending = s.sending.lock().await;
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
    let message = {
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
                d.reassembler.push(&payload)?
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
    if let Some(text) = message {
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
            if d.inbox.len() >= 128
                || d.inbox.iter().map(|m| m.text.len()).sum::<usize>() + text.len()
                    > 8 * 1024 * 1024
            {
                return Err(AppError::new(
                    "inbox_full",
                    "message",
                    "Переполнен буфер входящих сообщений",
                    true,
                ));
            }
            d.next_message_id += 1;
            let id = d.next_message_id;
            d.inbox.push(super::state::ReceivedMessage {
                generation: s.generation,
                id: format!("received-{}-{id}", s.generation),
                text: text.clone(),
            });
        }
        s.emit(
            "ssc-chat-message",
            serde_json::json!({"generation":s.generation,"text":text}),
        );
        crate::diagnostics::record(s.generation, "chat", "message", "received", None);
    }
    s.publish();
    Ok(())
}
pub async fn send_encrypted(s: &Arc<Session>, kind: u8, payload: &[u8]) -> Result<(), AppError> {
    s.ensure_active()?;
    let (dc, frame) = {
        let mut d = lock(&s.data);
        let dc = d.dc.clone().filter(|_| d.dc_open).ok_or_else(|| {
            AppError::new("channel_closed", "data_channel", "Канал недоступен", true)
        })?;
        let crypto = d
            .crypto
            .as_mut()
            .ok_or_else(|| AppError::new("key_required", "crypto", "Нет ключей сессии", false))?;
        (dc, crypto.encrypt(kind, payload)?)
    };
    dc.send(&Bytes::from(frame)).await.map_err(|_| {
        AppError::new(
            "send_failed",
            "message",
            "Не удалось передать сообщение каналу",
            true,
        )
    })?;
    s.ensure_active()
}
pub async fn confirm(
    s: &Arc<Session>,
    generation: u64,
    expected_sas: &str,
) -> Result<bool, AppError> {
    let _sending = s.sending.lock().await;
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
    let _sending = s.sending.lock().await;
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
