//! Flashing a Cavway X1 from the app (over Bluetooth): thin doors onto `cave_instrument::cavway_flash`, which holds
//! the protocol and is tested natively. One flash at a time, kept here; every call answers JSON.

use cave_instrument::cavway_flash::{self as fl, Flasher, Image, Phase, Step};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;

thread_local!(static FLASH: RefCell<Option<Flasher>> = const { RefCell::new(None) });

fn step_json(step: Step) -> String {
    match step {
        Step::Send { bytes, wait_ms } => serde_json::json!({"send": bytes, "wait_ms": wait_ms}).to_string(),
        Step::Done => serde_json::json!({"done": true}).to_string(),
        Step::Failed(e) => {
            serde_json::json!({"failed": e.to_string(), "app_untouched": e.app_untouched()}).to_string()
        }
    }
}

/// What an update file is: `{version, built, bytes, packets, warnings}` or `{error}`.
#[wasm_bindgen]
pub fn x1fw_inspect(bytes: &[u8]) -> String {
    match Image::parse(bytes) {
        Ok(img) => serde_json::json!({
            "version": img.version_text(), "built": img.built, "bytes": img.payload.len(),
            "packets": img.packets(), "warnings": img.warnings,
        })
        .to_string(),
        Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
    }
}

/// The bootloader's "who is there" (`J`): only an X1 in boot mode answers, with `4A 0A 00`.
#[wasm_bindgen]
pub fn x1fw_hello_frame() -> Vec<u8> {
    fl::hello_frame()
}

/// The bootloader version in `bytes`, or -1 if they hold no answer to `J`.
#[wasm_bindgen]
pub fn x1fw_hello_answer(bytes: &[u8]) -> i32 {
    match fl::parse_boot_reply(bytes) {
        Some((fl::BootReply::Hello { version }, _)) => i32::from(version),
        _ => -1,
    }
}

/// Starts a flash of `bytes` (an update file): the first step, or `{failed}` if the file is not usable.
#[wasm_bindgen]
pub fn x1fw_begin(bytes: &[u8]) -> String {
    let img = match Image::parse(bytes) {
        Ok(i) => i,
        Err(e) => return serde_json::json!({"failed": e.to_string(), "app_untouched": true}).to_string(),
    };
    let mut f = Flasher::new(img);
    let step = f.begin();
    FLASH.with(|c| *c.borrow_mut() = Some(f));
    step_json(step)
}

/// Bytes from the X1: the next step, or `null` while an answer is still arriving.
#[wasm_bindgen]
pub fn x1fw_bytes(bytes: &[u8]) -> String {
    FLASH.with(|c| match c.borrow_mut().as_mut().and_then(|f| f.on_bytes(bytes)) {
        Some(s) => step_json(s),
        None => "null".into(),
    })
}

/// Nothing arrived in time: the next step.
#[wasm_bindgen]
pub fn x1fw_timeout() -> String {
    FLASH.with(|c| match c.borrow_mut().as_mut() {
        Some(f) => step_json(f.on_timeout()),
        None => serde_json::json!({"failed": "no flash under way", "app_untouched": true}).to_string(),
    })
}

/// `{done, total, phase, resends}` for the flash under way.
#[wasm_bindgen]
pub fn x1fw_progress() -> String {
    FLASH.with(|c| match c.borrow().as_ref() {
        Some(f) => {
            let (done, total) = f.progress();
            let phase = match f.phase() {
                Phase::Idle => "idle",
                Phase::Erasing => "erasing",
                Phase::Sending => "sending",
                Phase::Installing => "installing",
                Phase::Done => "done",
                Phase::Failed => "failed",
            };
            serde_json::json!({"done": done, "total": total, "phase": phase, "resends": f.resends()}).to_string()
        }
        None => "null".into(),
    })
}
