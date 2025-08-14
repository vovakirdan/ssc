use crate::logger::emit_disconnected;
use crate::logger::log;
use crate::peer::crypto::u64_to_nonce;
use crate::peer::state::{
    COLLECTING_CANDIDATES, CRYPTO, DATA_CH, DISCONNECT_TASK, LOCAL_CANDIDATES, MY_PRIV, MY_PUB,
    PEER, PENDING_REMOTE_CANDIDATES, SAS_CONFIRMED, WAS_CONNECTED,
};
use bytes::Bytes;
use chacha20poly1305::aead::Aead;
use tauri::command;

/// Максимальный размер медиа (в байтах)
const MAX_MEDIA_BYTES: u64 = 10 * 1024 * 1024; // 10MB

/// текст по каналу
#[command]
pub async fn send_text(text: String) -> bool {
    log(&format!("send_text called with: {}", text));
    
    // Проверяем, подтвержден ли SAS пользователем
    let sas_confirmed = *SAS_CONFIRMED.lock().unwrap();
    let crypto_exists = CRYPTO.lock().unwrap().is_some();
    let data_ch_exists = DATA_CH.lock().unwrap().is_some();
    
    log(&format!(
        "send_text state check: sas_confirmed={}, crypto_exists={}, data_ch_exists={}",
        sas_confirmed, crypto_exists, data_ch_exists
    ));
    
    if !sas_confirmed {
        log("SAS not confirmed by user, not sending message");
        return false;
    }
    
    let dc = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = dc {
        // Получаем данные из мьютекса и освобождаем его
        let result = {
            let mut crypto_guard = CRYPTO.lock().unwrap();
            if let Some(ref mut ctx) = *crypto_guard {
                let seq_num = ctx.send_n;
                let nonce = u64_to_nonce(seq_num);
                ctx.send_n += 1;

                let plaintext = text.into_bytes();
                match ctx.sealing.encrypt(&nonce, plaintext.as_ref()) {
                    Ok(ciphertext) => {
                        log(&format!(
                            "Encrypted message with seq {}, length: {}",
                            seq_num,
                            ciphertext.len()
                        ));
                        Some(ciphertext)
                    }
                    Err(_) => {
                        log("Encryption failed");
                        None
                    }
                }
            } else {
                log("No crypto context available for sending");
                None
            }
        }; // мьютекс освобождается здесь

        if let Some(ciphertext) = result {
            let send_result = dc.send(&Bytes::from(ciphertext)).await.is_ok();
            log(&format!("Send result: {}", send_result));
            return send_result;
        }
    }
    log("No data channel available for sending");
    false
}

/// Метаданные начала передачи медиа
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MediaMeta {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub total_chunks: u32,
}

/// Чанк медиа-данных
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MediaChunk {
    pub id: String,
    pub index: u32,
    pub data: String, // base64-часть
}

/// Завершение передачи медиа
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MediaEnd {
    pub id: String,
}

/// отправка управляющего сообщения в data-channel (общий помощник)
async fn send_control_message(payload: String) -> bool {
    // Проверяем, подтвержден ли SAS пользователем
    let sas_confirmed = *SAS_CONFIRMED.lock().unwrap();
    let crypto_exists = CRYPTO.lock().unwrap().is_some();
    let data_ch_exists = DATA_CH.lock().unwrap().is_some();

    log(&format!(
        "send_control_message state check: sas_confirmed={}, crypto_exists={}, data_ch_exists={}",
        sas_confirmed, crypto_exists, data_ch_exists
    ));

    if !sas_confirmed {
        log("SAS not confirmed by user, not sending control message");
        return false;
    }

    let dc = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = dc {
        let result = {
            let mut crypto_guard = CRYPTO.lock().unwrap();
            if let Some(ref mut ctx) = *crypto_guard {
                let seq_num = ctx.send_n;
                let nonce = u64_to_nonce(seq_num);
                ctx.send_n += 1;

                let plaintext = payload.into_bytes();
                match ctx.sealing.encrypt(&nonce, plaintext.as_ref()) {
                    Ok(ciphertext) => {
                        log(&format!(
                            "Encrypted control message with seq {}, length: {}",
                            seq_num,
                            ciphertext.len()
                        ));
                        Some(ciphertext)
                    }
                    Err(_) => {
                        log("Encryption failed (control)");
                        None
                    }
                }
            } else {
                log("No crypto context available for sending (control)");
                None
            }
        };

        if let Some(ciphertext) = result {
            let send_result = dc.send(&bytes::Bytes::from(ciphertext)).await.is_ok();
            log(&format!("Send result (control): {}", send_result));
            return send_result;
        }
    }
    log("No data channel available for sending (control)");
    false
}

/// начало передачи медиа: отправляет метаданные
#[command]
pub async fn send_media_start(id: String, name: String, mime: String, size: u64, total_chunks: u32) -> bool {
    log(&format!(
        "send_media_start id={} name={} mime={} size={} total_chunks={}",
        id, name, mime, size, total_chunks
    ));

    if size > MAX_MEDIA_BYTES {
        log("Media exceeds MAX_MEDIA_BYTES, rejecting");
        return false;
    }

    let meta = MediaMeta { id, name, mime, size, total_chunks };
    let json = match serde_json::to_string(&meta) {
        Ok(s) => s,
        Err(e) => {
            log(&format!("Failed to serialize MediaMeta: {:?}", e));
            return false;
        }
    };
    let payload = format!("MEDIA_META:{}", json);
    send_control_message(payload).await
}

/// отправка чанка медиа (base64)
#[command]
pub async fn send_media_chunk(id: String, index: u32, data: String) -> bool {
    // Простейшая защита от слишком больших чанков, чтобы не переполнить буфер
    if data.len() > 128 * 1024 { // ~128KB символов base64
        log("Media chunk too large (>128KB base64), rejecting");
        return false;
    }

    let chunk = MediaChunk { id, index, data };
    let json = match serde_json::to_string(&chunk) {
        Ok(s) => s,
        Err(e) => {
            log(&format!("Failed to serialize MediaChunk: {:?}", e));
            return false;
        }
    };
    let payload = format!("MEDIA_CHUNK:{}", json);
    send_control_message(payload).await
}

/// завершение передачи медиа
#[command]
pub async fn send_media_end(id: String) -> bool {
    let end = MediaEnd { id };
    let json = match serde_json::to_string(&end) {
        Ok(s) => s,
        Err(e) => {
            log(&format!("Failed to serialize MediaEnd: {:?}", e));
            return false;
        }
    };
    let payload = format!("MEDIA_END:{}", json);
    send_control_message(payload).await
}

/// подтверждение SAS пользователем
#[command]
pub fn confirm_sas() -> bool {
    log("confirm_sas called - user confirmed SAS");
    *SAS_CONFIRMED.lock().unwrap() = true;
    
    // Отправляем событие подключения после подтверждения SAS
    use crate::logger::emit_connected;
    emit_connected();
    
    log("SAS confirmed, connection is now fully established");
    true
}

/// отклонение SAS пользователем
#[command]
pub async fn reject_sas() {
    log("reject_sas called - user rejected SAS");
    // Сбрасываем соединение при отклонении SAS
    disconnect().await;
}

/// получение fingerprint
#[command]
pub fn get_fingerprint() -> Option<String> {
    let crypto_guard = CRYPTO.lock().unwrap();
    let result = crypto_guard.as_ref().map(|c| {
        log(&format!("Found crypto context with SAS: {}", c.sas));
        c.sas.clone()
    });
    log(&format!(
        "get_fingerprint called, crypto exists: {}, result: {:?}",
        crypto_guard.is_some(),
        result
    ));
    result
}

/// проверка готовности соединения
#[command]
pub fn is_connected() -> bool {
    let crypto_exists = CRYPTO.lock().unwrap().is_some();
    let sas_confirmed = *SAS_CONFIRMED.lock().unwrap();
    let result = crypto_exists && sas_confirmed;
    
    log(&format!(
        "is_connected: crypto_exists={}, sas_confirmed={}, result={}",
        crypto_exists, sas_confirmed, result
    ));
    
    result
}

/// проверка состояния SAS
#[command]
pub fn get_sas_status() -> bool {
    let sas_confirmed = *SAS_CONFIRMED.lock().unwrap();
    log(&format!("get_sas_status: SAS_CONFIRMED={}", sas_confirmed));
    sas_confirmed
}

/// ручное разъединение
#[command]
pub async fn disconnect() {
    // извлекаем data channel и освобождаем мьютекс
    let dc = DATA_CH.lock().unwrap().take();
    if let Some(dc) = dc {
        let _ = dc.close().await;
    }

    // извлекаем peer connection и освобождаем мьютекс
    let pc = PEER.lock().unwrap().take();
    if let Some(pc) = pc {
        let _ = pc.close().await;
    }

    // отменяем отложенный disconnect, если он был
    if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
        log("Aborting pending disconnect task in manual disconnect");
        handle.abort();
    }

    // очищаем криптографический контекст и ключи
    log("Clearing CRYPTO context in disconnect");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    *WAS_CONNECTED.lock().unwrap() = false;

    // очищаем отложенные кандидаты
    PENDING_REMOTE_CANDIDATES.lock().unwrap().clear();

    // очищаем локальные кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = false;

    // отправляем событие отключения
    emit_disconnected();
}
