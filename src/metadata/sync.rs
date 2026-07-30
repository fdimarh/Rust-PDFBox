use crate::metadata::xmp::{XmpFields, build_basic_xmp};
use crate::{Document, PdfResult};

#[derive(Debug, Clone, Copy)]
pub struct SyncPolicy {
    pub title: bool,
    pub author: bool,
    pub subject: bool,
    pub keywords: bool,
    pub creator: bool,
    pub producer: bool,
    pub creation_date: bool,
    pub mod_date: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        Self {
            title: true,
            author: true,
            subject: true,
            keywords: true,
            creator: true,
            producer: true,
            creation_date: false,
            mod_date: false,
        }
    }
}

impl SyncPolicy {
    pub fn all_fields() -> Self {
        Self {
            creation_date: true,
            mod_date: true,
            ..Self::default()
        }
    }
}

pub fn sync_docinfo_to_xmp(doc: &mut Document, policy: SyncPolicy) -> PdfResult<()> {
    let info = doc.document_info();
    let title = if policy.title {
        info.title().map(|s| s.into_owned())
    } else {
        None
    };
    let author = if policy.author {
        info.author().map(|s| s.into_owned())
    } else {
        None
    };
    let subject = if policy.subject {
        info.subject().map(|s| s.into_owned())
    } else {
        None
    };
    let keywords = if policy.keywords {
        info.keywords().map(|s| s.into_owned())
    } else {
        None
    };
    let creator = if policy.creator {
        info.creator().map(|s| s.into_owned())
    } else {
        None
    };
    let producer = if policy.producer {
        info.producer().map(|s| s.into_owned())
    } else {
        None
    };
    let creation_date = if policy.creation_date {
        info.creation_date().and_then(|s| pdf_date_to_xmp(&s))
    } else {
        None
    };
    let mod_date = if policy.mod_date {
        info.mod_date().and_then(|s| pdf_date_to_xmp(&s))
    } else {
        None
    };

    let xml = build_basic_xmp(XmpFields {
        title: title.as_deref(),
        creator: author.as_deref(),
        subject: subject.as_deref(),
        keywords: keywords.as_deref(),
        creator_tool: creator.as_deref(),
        producer: producer.as_deref(),
        create_date: creation_date.as_deref(),
        modify_date: mod_date.as_deref(),
    });

    doc.set_xmp_metadata_raw(&xml)
}

pub fn sync_xmp_to_docinfo(doc: &mut Document, policy: SyncPolicy) -> PdfResult<()> {
    let Some(xmp) = doc.xmp_metadata() else {
        return Ok(());
    };

    let mut info = doc.document_info_mut()?;

    if policy.title {
        if let Some(value) = xmp.dc_title() {
            info.set_title(value)?;
        }
    }

    if policy.author {
        if let Some(value) = xmp.dc_creator() {
            info.set_author(value)?;
        }
    }

    if policy.subject {
        if let Some(value) = xmp.dc_subject() {
            info.set_subject(value)?;
        }
    }

    if policy.keywords {
        if let Some(value) = xmp.pdf_keywords() {
            info.set_keywords(value)?;
        }
    }

    if policy.creator {
        if let Some(value) = xmp.xmp_creator_tool() {
            info.set_creator(value)?;
        }
    }

    if policy.producer {
        if let Some(value) = xmp.pdf_producer() {
            info.set_producer(value)?;
        }
    }

    if policy.creation_date {
        if let Some(value) = xmp.xmp_create_date() {
            info.set_creation_date(value)?;
        }
    }

    if policy.mod_date {
        if let Some(value) = xmp.xmp_modify_date() {
            info.set_mod_date(value)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_policy_default() {
        let p = SyncPolicy::default();
        assert!(p.title);
        assert!(p.author);
        assert!(p.subject);
        assert!(p.keywords);
        assert!(p.creator);
        assert!(p.producer);
        assert!(!p.creation_date);
        assert!(!p.mod_date);
    }

    #[test]
    fn sync_policy_all_fields() {
        let p = SyncPolicy::all_fields();
        assert!(p.title);
        assert!(p.creation_date);
        assert!(p.mod_date);
    }

    #[test]
    fn pdf_date_to_xmp_simple() {
        let result = pdf_date_to_xmp("D:20260730120000+07'00'").unwrap();
        assert_eq!(result, "2026-07-30T12:00:00+07:00");
    }

    #[test]
    fn pdf_date_to_xmp_utc_z() {
        let result = pdf_date_to_xmp("D:20260730120000Z").unwrap();
        assert_eq!(result, "2026-07-30T12:00:00Z");
    }

    #[test]
    fn pdf_date_to_xmp_no_timezone() {
        let result = pdf_date_to_xmp("D:20260730120000").unwrap();
        assert_eq!(result, "2026-07-30T12:00:00");
    }

    #[test]
    fn pdf_date_to_xmp_no_d_prefix() {
        let result = pdf_date_to_xmp("20260730120000+07'00'").unwrap();
        assert_eq!(result, "2026-07-30T12:00:00+07:00");
    }

    #[test]
    fn pdf_date_to_xmp_too_short_returns_none() {
        assert!(pdf_date_to_xmp("20").is_none());
    }

    #[test]
    fn pdf_date_to_xmp_year_only() {
        let result = pdf_date_to_xmp("D:2026").unwrap();
        assert_eq!(result, "2026-01-01T00:00:00");
    }

    #[test]
    fn pdf_date_to_xmp_negative_timezone() {
        let result = pdf_date_to_xmp("D:20260730120000-05'30'").unwrap();
        assert_eq!(result, "2026-07-30T12:00:00-05:30");
    }

    #[test]
    fn sync_policy_no_fields_set() {
        let p = SyncPolicy {
            title: false,
            author: false,
            subject: false,
            keywords: false,
            creator: false,
            producer: false,
            creation_date: false,
            mod_date: false,
        };
        assert!(!p.title);
        assert!(!p.author);
    }

    #[test]
    fn sync_policy_clone() {
        let a = SyncPolicy::all_fields();
        let b = a.clone();
        assert_eq!(a.title, b.title);
    }

    #[test]
    fn sync_policy_debug() {
        let p = SyncPolicy::default();
        let _ = format!("{:?}", p);
    }
}

pub(crate) fn pdf_date_to_xmp(input: &str) -> Option<String> {
    let mut s = input.trim();
    if let Some(stripped) = s.strip_prefix("D:") {
        s = stripped;
    }

    let bytes = s.as_bytes();
    if bytes.len() < 4 {
        return None;
    }

    let year = &s[0..4];
    let month = s.get(4..6).unwrap_or("01");
    let day = s.get(6..8).unwrap_or("01");
    let hour = s.get(8..10).unwrap_or("00");
    let minute = s.get(10..12).unwrap_or("00");
    let second = s.get(12..14).unwrap_or("00");

    let tz = if s.len() > 14 {
        let tz_char = s.as_bytes()[14] as char;
        if tz_char == 'Z' {
            Some("Z".to_string())
        } else if tz_char == '+' || tz_char == '-' {
            let sign = tz_char;
            let offset_hour = s.get(15..17).unwrap_or("00");
            let mut offset_minute = "00";
            if let Some(rest) = s.get(17..) {
                if rest.starts_with("'") && rest.len() >= 3 {
                    offset_minute = &rest[1..3];
                }
            }
            Some(format!("{sign}{offset_hour}:{offset_minute}"))
        } else {
            None
        }
    } else {
        None
    };

    let mut iso = format!(
        "{year}-{month}-{day}T{hour}:{minute}:{second}",
        year = year,
        month = month,
        day = day,
        hour = hour,
        minute = minute,
        second = second
    );

    if let Some(tz) = tz {
        iso.push_str(&tz);
    }

    Some(iso)
}
