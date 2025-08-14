use crate::commands::util_api::get_fingerprint;
use crate::logger::log;
use crate::logger::{emit_disconnected, emit_message, emit_sas_to_ui};
use crate::peer::crypto::{build_ctx, u64_to_nonce};
use crate::peer::state::{
    COLLECTING_CANDIDATES, CRYPTO, DATA_CH, DISCONNECT_TASK, LOCAL_CANDIDATES, MEDIA_BUFFERS,
    MEDIA_INFO, MY_PRIV, MY_PUB, PENDING_REMOTE_CANDIDATES, SAS_CONFIRMED, TAG_LEN, WAS_CONNECTED,
};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use bytes::Bytes;
use chacha20poly1305::aead::Aead;
use tauri::Emitter;
use ring::{agreement, rand as ring_rand};
use std::sync::Arc;
use webrtc::data_channel::RTCDataChannel;

/// общий обработчик data-channel
pub fn attach_dc(dc: &Arc<RTCDataChannel>) {
    log("attach_dc called - clearing old state");

    // отменяем отложенный disconnect, если он был
    if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
        log("Aborting pending disconnect task in attach_dc");
        handle.abort();
    }

    // Очищаем старое состояние перед созданием нового соединения
    log("Clearing CRYPTO context in attach_dc");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    *WAS_CONNECTED.lock().unwrap() = false;
    *SAS_CONFIRMED.lock().unwrap() = false; // Сбрасываем флаг подтверждения SAS

    // очищаем отложенные кандидаты
    PENDING_REMOTE_CANDIDATES.lock().unwrap().clear();

    // очищаем локальные кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = false;

    {
        *DATA_CH.lock().unwrap() = Some(dc.clone());
    }

    // Генерируем ключи сразу при создании data channel
    let rng = ring_rand::SystemRandom::new();
    let my_priv = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
    let my_pub = my_priv.compute_public_key().unwrap();
    let my_pub_bytes = <[u8; 32]>::try_from(my_pub.as_ref()).unwrap();
    *MY_PRIV.lock().unwrap() = Some(my_priv);
    *MY_PUB.lock().unwrap() = Some(my_pub_bytes);
    log(&format!(
        "Generated pub key: {}",
        hex::encode(my_pub.as_ref())
    ));

    // Отправляем наш pub-key когда data channel открыт
    dc.on_open(Box::new({
        let dc = dc.clone();
        move || {
            log("Data channel opened, sending pub key...");
            tauri::async_runtime::spawn({
                let dc = dc.clone();
                async move {
                    // Отправляем публичный ключ в новом формате: PUBKEY:{base64_encoded_key}
                    let pubkey_b64 = STANDARD.encode(my_pub.as_ref());
                    let msg = format!("PUBKEY:{}", pubkey_b64);
                    let _result = dc.send(&Bytes::from(msg)).await;
                    log(&format!("Sent pub key: {}", hex::encode(my_pub.as_ref())));
                }
            });
            Box::pin(async {})
        }
    }));

    dc.on_message(Box::new(|msg| {
        log(&format!("Received message, length: {}", msg.data.len()));

        // Пробуем разобрать как строку для обработки PUBKEY/CONTROL сообщений
        if let Ok(text) = std::str::from_utf8(&msg.data) {
            if let Some(rest) = text.strip_prefix("PUBKEY:") {
                // Декодируем base64
                if let Ok(pub_bytes) = STANDARD.decode(rest) {
                    if pub_bytes.len() == 32 {
                        let mut peer_pub = [0u8; 32];
                        peer_pub.copy_from_slice(&pub_bytes);
                        log(&format!("Received pub key: {}", hex::encode(&peer_pub)));

                        // Проверяем, не создали ли мы уже криптографический контекст
                        if CRYPTO.lock().unwrap().is_some() {
                            log("Crypto context already exists, skipping...");
                            return Box::pin(async {});
                        }

                        // Строим криптографический контекст
                        let ctx = build_ctx(&peer_pub);
                        let sas = ctx.sas.clone();
                        log(&format!("SAS generated: {}", sas));
                        *CRYPTO.lock().unwrap() = Some(ctx);

                        // Отправляем SAS на UI для подтверждения пользователем
                        emit_sas_to_ui(&sas);
                        log("SAS sent to UI for user confirmation");

                        // НЕ отправляем событие подключения сразу - ждем подтверждения SAS
                        log("Crypto context established, waiting for SAS confirmation");

                        // Проверим, что fingerprint доступен сразу после создания контекста
                        let _test_fp = get_fingerprint();
                        log(&format!(
                            "Fingerprint immediately after context creation: {:?}",
                            _test_fp
                        ));

                        return Box::pin(async {});
                    }
                }
            }

            // Обработка управляющих сообщений медиа
            if text.starts_with("MEDIA_META:") || text.starts_with("MEDIA_CHUNK:") || text.starts_with("MEDIA_END:") {
                if !*SAS_CONFIRMED.lock().unwrap() {
                    log("SAS not confirmed by user, ignoring media control message");
                    return Box::pin(async {});
                }

                if text.starts_with("MEDIA_META:") {
                    let json = &text[11..];
                    match serde_json::from_str::<crate::commands::util_api::MediaMeta>(json) {
                        Ok(meta) => {
                            // Проверка размера
                            if meta.size > 10 * 1024 * 1024 {
                                log("Incoming media too large (>10MB), ignoring");
                                return Box::pin(async {});
                            }
                            MEDIA_BUFFERS.lock().unwrap().insert(meta.id.clone(), Vec::with_capacity(meta.size as usize));
                            MEDIA_INFO.lock().unwrap().insert(meta.id.clone(), (meta.name, meta.mime, meta.size, meta.total_chunks, 0));
                            log("MEDIA_META stored");
                        }
                        Err(e) => log(&format!("Failed to parse MEDIA_META: {:?}", e)),
                    }
                    return Box::pin(async {});
                }

                if text.starts_with("MEDIA_CHUNK:") {
                    let json = &text[12..];
                    match serde_json::from_str::<crate::commands::util_api::MediaChunk>(json) {
                        Ok(chunk) => {
                            use base64::Engine;
                            let engine = base64::engine::general_purpose::STANDARD;
                            match engine.decode(chunk.data.as_bytes()) {
                                Ok(bytes) => {
                                    if let Some(buf) = MEDIA_BUFFERS.lock().unwrap().get_mut(&chunk.id) {
                                        buf.extend_from_slice(&bytes);
                                        if let Some(info) = MEDIA_INFO.lock().unwrap().get_mut(&chunk.id) {
                                            info.4 += 1; // received_chunks
                                        }
                                    }
                                }
                                Err(e) => log(&format!("Failed to decode MEDIA_CHUNK base64: {:?}", e)),
                            }
                        }
                        Err(e) => log(&format!("Failed to parse MEDIA_CHUNK: {:?}", e)),
                    }
                    return Box::pin(async {});
                }

                if text.starts_with("MEDIA_END:") {
                    let json = &text[10..];
                    match serde_json::from_str::<crate::commands::util_api::MediaEnd>(json) {
                        Ok(end) => {
                            let buf_opt = MEDIA_BUFFERS.lock().unwrap().remove(&end.id);
                            let info_opt = MEDIA_INFO.lock().unwrap().remove(&end.id);
                            if let (Some(bytes), Some((name, mime, size, total_chunks, received_chunks))) = (buf_opt, info_opt) {
                                if bytes.len() as u64 != size {
                                    log(&format!("MEDIA size mismatch: expected {}, got {}", size, bytes.len()));
                                }
                                if received_chunks != total_chunks {
                                    log(&format!("MEDIA chunks mismatch: expected {}, got {}", total_chunks, received_chunks));
                                }

                                // Отправляем событие в UI с base64 полезной нагрузкой
                                if let Some(app) = crate::peer::state::APP.lock().unwrap().clone() {
                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                    let payload = serde_json::json!({
                                        "id": end.id,
                                        "name": name,
                                        "mime": mime,
                                        "size": size,
                                        "data": b64,
                                    });
                                    let _ = app.emit("ssc-media", payload);
                                }
                                log("MEDIA_END emitted to UI");
                            } else {
                                log("MEDIA_END without buffers/info");
                            }
                        }
                        Err(e) => log(&format!("Failed to parse MEDIA_END: {:?}", e)),
                    }
                    return Box::pin(async {});
                }
            }
        }

        // ----- иначе зашифрованное сообщение -----
        // Проверяем, подтвержден ли SAS пользователем
        if !*SAS_CONFIRMED.lock().unwrap() {
            log("SAS not confirmed by user, ignoring encrypted message");
            return Box::pin(async {});
        }

        let mut lock = CRYPTO.lock().unwrap();
        if let Some(ref mut ctx) = *lock {
            if msg.data.len() < TAG_LEN {
                log(&format!(
                    "Message too short: {} < {}",
                    msg.data.len(),
                    TAG_LEN
                ));
                return Box::pin(async {});
            }

            let nonce = u64_to_nonce(ctx.recv_n);
            let ciphertext = &msg.data[..];

            match ctx.opening.decrypt(&nonce, ciphertext) {
                Ok(plaintext) => {
                    // Простая защита от replay: проверяем что sequence number больше последнего принятого
                    if ctx.recv_n > ctx.last_accepted_recv {
                        // Обновляем последний принятый sequence number
                        ctx.last_accepted_recv = ctx.recv_n;
                        ctx.recv_n += 1;

                        let plain = String::from_utf8_lossy(&plaintext).to_string();
                        log(&format!("Decrypted message: {}", plain));

                        // Проверяем управляющие префиксы для медиа
                        if plain.starts_with("MEDIA_META:") {
                            let json = &plain[11..];
                            match serde_json::from_str::<crate::commands::util_api::MediaMeta>(json) {
                                Ok(meta) => {
                                    if meta.size > 10 * 1024 * 1024 {
                                        log("Incoming media too large (>10MB), ignoring");
                                    } else {
                                        MEDIA_BUFFERS.lock().unwrap().insert(meta.id.clone(), Vec::with_capacity(meta.size as usize));
                                        MEDIA_INFO.lock().unwrap().insert(meta.id.clone(), (meta.name, meta.mime, meta.size, meta.total_chunks, 0));
                                        log("MEDIA_META stored (encrypted)");
                                    }
                                }
                                Err(e) => log(&format!("Failed to parse MEDIA_META (enc): {:?}", e)),
                            }
                        } else if plain.starts_with("MEDIA_CHUNK:") {
                            let json = &plain[12..];
                            match serde_json::from_str::<crate::commands::util_api::MediaChunk>(json) {
                                Ok(chunk) => {
                                    use base64::Engine;
                                    let engine = base64::engine::general_purpose::STANDARD;
                                    match engine.decode(chunk.data.as_bytes()) {
                                        Ok(bytes) => {
                                            if let Some(buf) = MEDIA_BUFFERS.lock().unwrap().get_mut(&chunk.id) {
                                                buf.extend_from_slice(&bytes);
                                                if let Some(info) = MEDIA_INFO.lock().unwrap().get_mut(&chunk.id) {
                                                    info.4 += 1;
                                                }
                                            }
                                        }
                                        Err(e) => log(&format!("Failed to decode MEDIA_CHUNK base64 (enc): {:?}", e)),
                                    }
                                }
                                Err(e) => log(&format!("Failed to parse MEDIA_CHUNK (enc): {:?}", e)),
                            }
                        } else if plain.starts_with("MEDIA_END:") {
                            let json = &plain[10..];
                            match serde_json::from_str::<crate::commands::util_api::MediaEnd>(json) {
                                Ok(end) => {
                                    let buf_opt = MEDIA_BUFFERS.lock().unwrap().remove(&end.id);
                                    let info_opt = MEDIA_INFO.lock().unwrap().remove(&end.id);
                                    if let (Some(bytes), Some((name, mime, size, total_chunks, received_chunks))) = (buf_opt, info_opt) {
                                        if bytes.len() as u64 != size {
                                            log(&format!("MEDIA size mismatch: expected {}, got {}", size, bytes.len()));
                                        }
                                        if received_chunks != total_chunks {
                                            log(&format!("MEDIA chunks mismatch: expected {}, got {}", total_chunks, received_chunks));
                                        }
                                        if let Some(app) = crate::peer::state::APP.lock().unwrap().clone() {
                                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                            let payload = serde_json::json!({
                                                "id": end.id,
                                                "name": name,
                                                "mime": mime,
                                                "size": size,
                                                "data": b64,
                                            });
                                            let _ = app.emit("ssc-media", payload);
                                        }
                                        log("MEDIA_END emitted to UI (enc)");
                                    } else {
                                        log("MEDIA_END without buffers/info (enc)");
                                    }
                                }
                                Err(e) => log(&format!("Failed to parse MEDIA_END (enc): {:?}", e)),
                            }
                        } else {
                            // Обычный текст
                            emit_message(&plain);
                        }
                    } else {
                        log(&format!(
                            "Replay attack detected: received seq {} <= last accepted seq {}",
                            ctx.recv_n, ctx.last_accepted_recv
                        ));
                    }
                }
                Err(_) => {
                    log(&format!(
                        "Failed to decrypt message with seq {}",
                        ctx.recv_n
                    ));
                }
            }
        } else {
            log("No crypto context available for message decryption");
        }
        Box::pin(async {})
    }));

    dc.on_close(Box::new(|| {
        log("Data channel closed - emitting disconnected");
        emit_disconnected();
        Box::pin(async {})
    }));
}
