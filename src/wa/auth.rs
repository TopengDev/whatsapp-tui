/// Auth flow handling — QR display, pairing code input.
/// Stubbed until whatsapp-rust is integrated.
use anyhow::Result;

/// Process QR code data for display in the TUI overlay.
pub fn format_qr_data(data: &str) -> Result<Vec<String>> {
    // TODO: actual QR generation via qrcode crate for terminal display
    let lines = vec![
        format!("QR Code Data: {}", &data[..data.len().min(32)]),
        "Scan with WhatsApp on your phone".to_string(),
    ];
    Ok(lines)
}

/// Process pairing code for display.
pub fn format_pairing_code(code: &str) -> String {
    // Format as XXXX-XXXX for readability
    if code.len() == 8 {
        format!("{}-{}", &code[..4], &code[4..])
    } else {
        code.to_string()
    }
}
