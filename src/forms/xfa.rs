use crate::cos::{CosDictionary, CosName, CosObject};
use crate::io;
use crate::ObjectStore;

/// A single XFA packet.
///
/// In array-based `/XFA`, packets are usually stored as pairs:
/// `(packet-name, packet-stream)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfaPacket {
	name: Option<String>,
	xml: Vec<u8>,
}

impl XfaPacket {
	pub fn name(&self) -> Option<&str> {
		self.name.as_deref()
	}

	pub fn xml(&self) -> &[u8] {
		&self.xml
	}
}

/// Read-only XFA view extracted from `/AcroForm /XFA`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct XfaForm {
	packets: Vec<XfaPacket>,
}

impl XfaForm {
	pub fn packets(&self) -> &[XfaPacket] {
		&self.packets
	}

	/// Returns packet names in document order, skipping unnamed packets.
	pub fn packet_names(&self) -> Vec<&str> {
		self.packets.iter().filter_map(|p| p.name()).collect()
	}

	/// Returns a packet by case-insensitive packet name.
	pub fn packet(&self, name: &str) -> Option<&XfaPacket> {
		self.packets
			.iter()
			.find(|p| p.name().map(|n| n.eq_ignore_ascii_case(name)).unwrap_or(false))
	}

	/// Returns the `/XFA` `datasets` packet XML when present.
	pub fn datasets_xml(&self) -> Option<&[u8]> {
		self.packet("datasets").map(|p| p.xml())
	}

	pub fn is_empty(&self) -> bool {
		self.packets.is_empty()
	}

	/// Concatenates all packet XML payloads in order.
	pub fn raw_xml(&self) -> Vec<u8> {
		let total: usize = self.packets.iter().map(|p| p.xml.len()).sum();
		let mut out = Vec::with_capacity(total);
		for packet in &self.packets {
			out.extend_from_slice(&packet.xml);
		}
		out
	}

	pub(crate) fn from_acro_form_dict(dict: &CosDictionary, store: &ObjectStore) -> Option<Self> {
		let xfa = dict.get(&CosName::new(b"XFA".to_vec()))?;
		let packets = parse_xfa_packets(xfa, store);
		Some(Self { packets })
	}
}

fn parse_xfa_packets(xfa_obj: &CosObject, store: &ObjectStore) -> Vec<XfaPacket> {
	match resolve_object(xfa_obj, store) {
		Some(CosObject::Array(items)) => parse_xfa_packet_array(items, store),
		Some(obj) => {
			if let Some(xml) = decode_xfa_payload(obj, store) {
				vec![XfaPacket {
					name: None,
					xml,
				}]
			} else {
				Vec::new()
			}
		}
		None => Vec::new(),
	}
}

fn parse_xfa_packet_array(items: &[CosObject], store: &ObjectStore) -> Vec<XfaPacket> {
	let mut packets = Vec::new();
	let mut i = 0;

	while i < items.len() {
		let current = resolve_object(&items[i], store).unwrap_or(&items[i]);

		if let Some(name_bytes) = current.as_string() {
			let name = Some(String::from_utf8_lossy(name_bytes).to_string());
			if i + 1 < items.len() {
				if let Some(xml) = decode_xfa_payload(&items[i + 1], store) {
					packets.push(XfaPacket { name, xml });
					i += 2;
					continue;
				}
			}
			i += 1;
			continue;
		}

		if let Some(xml) = decode_xfa_payload(&items[i], store) {
			packets.push(XfaPacket { name: None, xml });
		}
		i += 1;
	}

	packets
}

fn decode_xfa_payload(obj: &CosObject, store: &ObjectStore) -> Option<Vec<u8>> {
	match resolve_object(obj, store)? {
		CosObject::Stream(stream) => {
			let filter = stream.dictionary.get(&CosName::new(b"Filter".to_vec()));
			io::decode_stream(&stream.data, filter).ok()
		}
		CosObject::String(s) => Some(s.clone()),
		_ => None,
	}
}

fn resolve_object<'a>(obj: &'a CosObject, store: &'a ObjectStore) -> Option<&'a CosObject> {
	match obj {
		CosObject::Reference(id) => store.get(id),
		_ => Some(obj),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cos::{CosStream, ObjectId};

	#[test]
	fn xfa_from_single_stream() {
		let mut store = ObjectStore::new();
		let mut form = CosDictionary::new();

		let stream = CosStream::new(CosDictionary::new(), b"<xfa>single</xfa>".to_vec());
		store.insert(ObjectId::new(10, 0), CosObject::Stream(stream));
		form.insert(
			CosName::new(b"XFA".to_vec()),
			CosObject::Reference(ObjectId::new(10, 0)),
		);

		let xfa = XfaForm::from_acro_form_dict(&form, &store).unwrap();
		assert_eq!(xfa.packets().len(), 1);
		assert_eq!(xfa.packets()[0].name(), None);
		assert_eq!(xfa.packets()[0].xml(), b"<xfa>single</xfa>");
		assert_eq!(xfa.raw_xml(), b"<xfa>single</xfa>");
	}

	#[test]
	fn xfa_from_packet_array() {
		let mut store = ObjectStore::new();
		let mut form = CosDictionary::new();

		let template = CosStream::new(CosDictionary::new(), b"<template/>".to_vec());
		let datasets = CosStream::new(CosDictionary::new(), b"<datasets/>".to_vec());
		store.insert(ObjectId::new(20, 0), CosObject::Stream(template));
		store.insert(ObjectId::new(21, 0), CosObject::Stream(datasets));

		form.insert(
			CosName::new(b"XFA".to_vec()),
			CosObject::Array(vec![
				CosObject::String(b"template".to_vec()),
				CosObject::Reference(ObjectId::new(20, 0)),
				CosObject::String(b"datasets".to_vec()),
				CosObject::Reference(ObjectId::new(21, 0)),
			]),
		);

		let xfa = XfaForm::from_acro_form_dict(&form, &store).unwrap();
		assert_eq!(xfa.packets().len(), 2);
		assert_eq!(xfa.packets()[0].name(), Some("template"));
		assert_eq!(xfa.packets()[0].xml(), b"<template/>");
		assert_eq!(xfa.packets()[1].name(), Some("datasets"));
		assert_eq!(xfa.packets()[1].xml(), b"<datasets/>");
		assert_eq!(xfa.packet_names(), vec!["template", "datasets"]);
		assert_eq!(xfa.datasets_xml(), Some(b"<datasets/>".as_slice()));
		assert_eq!(xfa.packet("Template").map(|p| p.xml()), Some(b"<template/>".as_slice()));
	}

	#[test]
	fn xfa_missing_or_malformed_is_handled() {
		let mut store = ObjectStore::new();

		let empty_form = CosDictionary::new();
		assert!(XfaForm::from_acro_form_dict(&empty_form, &store).is_none());

		let mut malformed_form = CosDictionary::new();
		malformed_form.insert(
			CosName::new(b"XFA".to_vec()),
			CosObject::Array(vec![
				CosObject::String(b"datasets".to_vec()),
				CosObject::Reference(ObjectId::new(99, 0)),
			]),
		);

		let xfa = XfaForm::from_acro_form_dict(&malformed_form, &store).unwrap();
		assert!(xfa.is_empty());

		store.insert(
			ObjectId::new(99, 0),
			CosObject::Stream(CosStream::new(CosDictionary::new(), b"<ok/>".to_vec())),
		);
		let xfa_ok = XfaForm::from_acro_form_dict(&malformed_form, &store).unwrap();
		assert_eq!(xfa_ok.packets().len(), 1);
		assert_eq!(xfa_ok.packets()[0].name(), Some("datasets"));
		assert_eq!(xfa_ok.packets()[0].xml(), b"<ok/>");
	}
}

