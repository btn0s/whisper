use arboard::Clipboard;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

const V_KEYCODE: CGKeyCode = 9;

pub fn paste_text(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;

    std::thread::sleep(std::time::Duration::from_millis(50));

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create CGEventSource")?;

    let key_down = CGEvent::new_keyboard_event(source.clone(), V_KEYCODE, true)
        .map_err(|_| "Failed to create key down event")?;
    key_down.set_flags(CGEventFlags::CGEventFlagCommand);

    let key_up = CGEvent::new_keyboard_event(source, V_KEYCODE, false)
        .map_err(|_| "Failed to create key up event")?;
    key_up.set_flags(CGEventFlags::CGEventFlagCommand);

    key_down.post(CGEventTapLocation::HID);
    key_up.post(CGEventTapLocation::HID);

    Ok(())
}
