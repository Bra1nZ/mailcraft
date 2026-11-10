// SPDX-FileCopyrightText: MailCraft contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//! While a request from the control channel is handled, the window is marked as driven by it,
//! and is as it was again afterwards, however the handling ends: an answer, a refusal, or a
//! panic in what handled it.

use std::panic::{AssertUnwindSafe, catch_unwind};

use mailcraft_engine::Session;
use serde_json::json;

use super::{ControlRequest, handle};
use crate::{MailApp, Services};

/// Wednesday 2026-10-14 16:00 UTC.
const NOW: i64 = 1_791_993_600;

/// The window, its host's file picker one that goes wrong: it panics.
fn app() -> MailApp {
    let mut app = MailApp::new(Session::at(mailcraft_engine::sample::mailbox(NOW, 0), NOW, 0), Services::default());
    app.autosave = false;
    app.services.open_async = Some(Box::new(|_, _| panic!("a host that went wrong")));
    app
}

fn request(method: &str, params: serde_json::Value) -> ControlRequest {
    ControlRequest::new(method, params).0
}

#[test]
fn a_request_marks_the_window_driven_only_while_it_is_handled() {
    let ctx = egui::Context::default();
    // Not driven before, or driven already (input the control channel injected in this frame).
    for before in [false, true] {
        let mut app = app();
        app.driven = before;
        for (method, params) in [("engine.execute", json!({"command": "mail.markAllRead"})), ("engine.execute", json!({})), ("no.such", json!(7))] {
            let _ = handle(&mut app, &ctx, &request(method, params));
            assert_eq!(app.driven, before, "{method}");
        }
        // A request whose handling panics: as it was too.
        let unwound = catch_unwind(AssertUnwindSafe(|| handle(&mut app, &ctx, &request("engine.execute", json!({"command": "ui.open"})))));
        assert!(unwound.is_err(), "the host's picker was not asked");
        assert_eq!(app.driven, before);
    }
}

mod screenshot_budget {
    use super::*;

    /// A screenshot whose PNG doesn't fit the inline budget is scaled down to it, and the
    /// answer says so: the scaled image rides in an authenticated reply line, which the MCP
    /// bridge caps at 8 MiB, so base64 of the PNG plus the envelope must stay under it.
    #[test]
    fn a_png_over_the_budget_is_scaled_to_fit() {
        // Noise hardly compresses, so a small image is enough to exceed a small budget.
        let size = 96;
        let mut pixels = Vec::with_capacity(size * size);
        for y in 0..size {
            for x in 0..size {
                pixels.push(egui::Color32::from_rgb((x * 7 % 256) as u8, (y * 13 % 256) as u8, (x * y % 256) as u8));
            }
        }
        let image = egui::ColorImage { size: [size, size], source_size: egui::Vec2::new(size as f32, size as f32), pixels };
        let r = super::super::inline_screenshot_within(&image, 1024);
        assert_eq!(r["ok"], json!(true), "{r}");
        assert_eq!(r["result"]["scaled"], json!(true), "{r}");
        assert!((r["result"]["width"].as_u64().unwrap_or(0)) < size as u64, "{r}");
        // The delivered PNG, encoded, stays under the budget the bridge reads with.
        let data = mailcraft_engine::cmd::base64_decode(r["result"]["data"].as_str().unwrap_or_default()).unwrap();
        assert!(data.len() <= 1024, "{r}");
    }

    /// A screenshot whose PNG fits is delivered as-is: no scaling, no changed dimensions.
    #[test]
    fn a_png_within_the_budget_is_not_scaled() {
        let image = egui::ColorImage { size: [4, 4], source_size: egui::Vec2::new(4.0, 4.0), pixels: vec![egui::Color32::BLACK; 16] };
        let r = super::super::inline_screenshot_within(&image, mailcraft_engine::cmd::INLINE_SCREENSHOT_PNG_BUDGET);
        assert_eq!(r["ok"], json!(true), "{r}");
        assert_eq!(r["result"]["scaled"], json!(false), "{r}");
        assert_eq!(r["result"]["width"].as_u64(), Some(4), "{r}");
        assert_eq!(r["result"]["height"].as_u64(), Some(4), "{r}");
    }
}
