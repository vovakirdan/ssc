use super::{
    crypto::KeyExchange,
    protocol::{KIND_CHUNK, KIND_READY},
    state::{lock, Phase, Session},
};
use crate::errors::AppError;
use bytes::Bytes;
use std::sync::Arc;
use webrtc::data_channel::{data_channel_state::RTCDataChannelState, RTCDataChannel};

pub fn attach_dc(s: &Arc<Session>, dc: &Arc<RTCDataChannel>) -> Result<(), AppError> {
    s.ensure_active()?;
    let exchange = KeyExchange::new()?;
    let public = exchange.public_key();
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
        d.exchange = Some(exchange);
        d.dc = Some(dc.clone());
    }
    let weak = Arc::downgrade(s);
    let weak_dc = Arc::downgrade(dc);
    dc.on_open(Box::new(move || {
        let weak = weak.clone();
        let weak_dc = weak_dc.clone();
        Box::pin(async move {
            if let (Some(s), Some(dc)) = (weak.upgrade(), weak_dc.upgrade()) {
                if !s.active() {
                    return;
                }
                let mut hello = vec![2, 0];
                hello.extend_from_slice(&public);
                if dc.send(&Bytes::from(hello)).await.is_err() {
                    s.error(&AppError::new(
                        "key_send",
                        "key_exchange",
                        "Не удалось отправить публичный ключ",
                        true,
                    ));
                    s.close_from_callback();
                } else {
                    crate::diagnostics::record(
                        s.generation,
                        "crypto",
                        "key_exchange",
                        "key_sent",
                        None,
                    );
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
                if let Err(error) = receive(&s, &msg.data) {
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
pub(super) fn receive(s: &Arc<Session>, bytes: &[u8]) -> Result<(), AppError> {
    if bytes.starts_with(&[2, 0]) {
        if bytes.len() != 34 {
            return Err(AppError::new(
                "key_length",
                "key_exchange",
                "Некорректный публичный ключ",
                false,
            ));
        }
        let peer = <[u8; 32]>::try_from(&bytes[2..])
            .map_err(|_| AppError::new("key_length", "key_exchange", "Некорректный ключ", false));
        let peer = peer?;
        let sas = {
            let mut d = lock(&s.data);
            if d.closed {
                return Err(AppError::new(
                    "session_closed",
                    "session",
                    "Сессия завершена",
                    true,
                ));
            }
            let exchange = d.exchange.take().ok_or_else(|| {
                AppError::new(
                    "duplicate_key",
                    "key_exchange",
                    "Повторный обмен ключами",
                    false,
                )
            })?;
            let crypto = exchange.finish(peer)?;
            let sas = crypto.sas.clone();
            d.crypto = Some(crypto);
            sas
        };
        s.emit("ssc-sas", sas);
        s.publish();
        return Ok(());
    }
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
        let dc =
            d.dc.clone()
                .filter(|dc| dc.ready_state() == RTCDataChannelState::Open)
                .ok_or_else(|| {
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
pub async fn confirm(s: &Arc<Session>) -> Result<bool, AppError> {
    let _sending = s.sending.lock().await;
    s.ensure_active()?;
    if lock(&s.data).local_ready {
        return Ok(true);
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
