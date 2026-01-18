use clap::Args;
use std::process::Command;

#[derive(Args)]
pub struct NotifyArgs {
    /// Notification message body
    message: String,

    /// Optional notification title
    #[arg(long)]
    title: Option<String>,

    /// Optional notification subtitle
    #[arg(long)]
    subtitle: Option<String>,
}

impl NotifyArgs {
    pub fn run(self) {
        let script = build_script(&self.message, self.title.as_deref(), self.subtitle.as_deref());

        let status = Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status();

        if let Err(err) = status {
            eprintln!("Failed to send notification: {err}");
        }
    }
}

fn build_script(message: &str, title: Option<&str>, subtitle: Option<&str>) -> String {
    let mut script = String::new();

    match title {
        Some(title) => {
            script.push_str("display notification ");
            script.push_str(&format!("{:?}", message));
            script.push_str(" with title ");
            script.push_str(&format!("{:?}", title));
        }
        None => {
            script.push_str("display notification ");
            script.push_str(&format!("{:?}", message));
        }
    }

    if let Some(subtitle) = subtitle {
        script.push_str(" subtitle ");
        script.push_str(&format!("{:?}", subtitle));
    }

    script
}

#[cfg(test)]
mod tests {
    use super::build_script;

    #[test]
    fn builds_notification_without_title_or_subtitle() {
        let script = build_script("Hi", None, None);
        assert_eq!(script, "display notification \"Hi\"");
    }

    #[test]
    fn builds_notification_with_title() {
        let script = build_script("Hi", Some("Kitchen"), None);
        assert_eq!(script, "display notification \"Hi\" with title \"Kitchen\"");
    }

    #[test]
    fn builds_notification_with_title_and_subtitle() {
        let script = build_script("Hi", Some("Kitchen"), Some("Ready"));
        assert_eq!(
            script,
            "display notification \"Hi\" with title \"Kitchen\" subtitle \"Ready\""
        );
    }

    #[test]
    fn quotes_are_escaped_for_applescript() {
        let script = build_script("Hi \"there\"", Some("Kit\"chen"), None);
        assert_eq!(
            script,
            "display notification \"Hi \\\"there\\\"\" with title \"Kit\\\"chen\""
        );
    }
}
