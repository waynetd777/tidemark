// Copyright (c) 2026 Wayne Davies
// SPDX-License-Identifier: GPL-3.0-or-later (see LICENSE at the repository root)

//! macOS's standard About panel, opened by hand so its credits can carry a link: Tauri's About
//! item passes the credits as plain text, which the panel shows but can't click. Called on the
//! main thread. Ported from Brainstead.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::{NSDictionary, NSString, NSURL};

/// NSTextAlignmentCenter on macOS (Left 0, Right 1, Center 2).
const CENTER: isize = 2;

/// Shows the panel: the name, version, copyright and icon (a PNG), with `link` under the copyright,
/// centred and clickable, opening `url`.
pub fn show(name: &str, version: &str, copyright: &str, icon_png: &[u8], link: &str, url: &str) {
    unsafe {
        let data: Retained<AnyObject> = msg_send![class!(NSData), dataWithBytes: icon_png.as_ptr().cast::<std::ffi::c_void>(), length: icon_png.len()];
        let image: Option<Retained<AnyObject>> =
            msg_send![msg_send![class!(NSImage), alloc], initWithData: &*data];

        let ns_url = NSURL::URLWithString(&NSString::from_str(url));
        let size: f64 = msg_send![class!(NSFont), smallSystemFontSize];
        let font: Retained<AnyObject> = msg_send![class!(NSFont), systemFontOfSize: size];
        let para: Retained<AnyObject> = msg_send![class!(NSMutableParagraphStyle), new];
        let _: () = msg_send![&*para, setAlignment: CENTER];
        let mut keys = vec![
            NSString::from_str("NSFont"),
            NSString::from_str("NSParagraphStyle"),
        ];
        let mut vals: Vec<&AnyObject> = vec![&font, &para];
        if let Some(u) = &ns_url {
            keys.push(NSString::from_str("NSLink"));
            vals.push(u.as_ref());
        }
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let attrs = NSDictionary::from_slices(&key_refs, &vals);
        let credits: Retained<AnyObject> = msg_send![msg_send![class!(NSAttributedString), alloc], initWithString: &*NSString::from_str(link), attributes: &*attrs];

        let name = NSString::from_str(name);
        let version = NSString::from_str(version);
        let copyright = NSString::from_str(copyright);
        let mut okeys = vec![
            NSString::from_str("ApplicationName"),
            NSString::from_str("ApplicationVersion"),
            NSString::from_str("Copyright"),
            NSString::from_str("Credits"),
        ];
        let mut ovals: Vec<&AnyObject> = vec![
            name.as_ref(),
            version.as_ref(),
            copyright.as_ref(),
            &credits,
        ];
        if let Some(img) = &image {
            okeys.push(NSString::from_str("ApplicationIcon"));
            ovals.push(img);
        }
        let okey_refs: Vec<&NSString> = okeys.iter().map(|k| &**k).collect();
        let options = NSDictionary::from_slices(&okey_refs, &ovals);
        let app: Retained<AnyObject> = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![&*app, activateIgnoringOtherApps: true];
        let _: () = msg_send![&*app, orderFrontStandardAboutPanelWithOptions: &*options];
    }
}
