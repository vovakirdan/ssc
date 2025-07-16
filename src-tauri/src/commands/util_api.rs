use crate::logger::emit_disconnected;
use crate::logger::log;
use crate::peer::crypto::u64_to_nonce;
use crate::peer::state::{
    COLLECTING_CANDIDATES, CRYPTO, DATA_CH, DISCONNECT_TASK, LOCAL_CANDIDATES, MY_PRIV, MY_PUB,
    PEER, PENDING_REMOTE_CANDIDATES, WAS_CONNECTED,
};
use bytes::Bytes;
use chacha20poly1305::aead::Aead;
use std::io::Read;
use tauri::command;

/// текст по каналу
#[command]
pub async fn send_text(text: String) -> bool {
    log(&format!("send_text called with: {}", text));
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
    CRYPTO.lock().unwrap().is_some()
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
