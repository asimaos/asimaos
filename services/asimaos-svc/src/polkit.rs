//! Authorisation through polkit. The service asks polkit about the *caller* of the D-Bus method;
//! polkit shows the password prompt itself when an action needs one.

use std::collections::HashMap;
use zbus::{fdo, message::Header, zvariant::Value, Connection};

/// Checks `action` for the sender of the current message. `enabled` is false only for a service
/// running on a private test bus (`--session`), never on the system bus.
pub async fn authorize(conn: &Connection, header: &Header<'_>, action: &str, enabled: bool) -> fdo::Result<()> {
    if !enabled {
        return Ok(());
    }
    // The person may take a while to type the password: the service must not exit meanwhile.
    let _pending = crate::idle::Pending::hold();
    let sender =
        header.sender().ok_or_else(|| fdo::Error::AccessDenied("The request has no sender".into()))?.to_string();
    let mut subject_details: HashMap<&str, Value<'_>> = HashMap::new();
    subject_details.insert("name", Value::from(sender.as_str()));
    let subject = ("system-bus-name", subject_details);
    let details: HashMap<&str, &str> = HashMap::new();
    const ALLOW_USER_INTERACTION: u32 = 1;
    let reply = conn
        .call_method(
            Some("org.freedesktop.PolicyKit1"),
            "/org/freedesktop/PolicyKit1/Authority",
            Some("org.freedesktop.PolicyKit1.Authority"),
            "CheckAuthorization",
            &(subject, action, details, ALLOW_USER_INTERACTION, ""),
        )
        .await
        .map_err(|e| fdo::Error::Failed(format!("polkit: {e}")))?;
    let (authorized, _challenge, _details): (bool, bool, HashMap<String, String>) =
        reply.body().deserialize().map_err(|e| fdo::Error::Failed(format!("polkit: {e}")))?;
    if authorized {
        Ok(())
    } else {
        Err(fdo::Error::AccessDenied("Not allowed".into()))
    }
}
