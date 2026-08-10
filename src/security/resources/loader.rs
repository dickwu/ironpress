use std::collections::HashMap;
use std::path::Path;

use crate::util::decode_base64;

use super::{DocumentResources, ResolvedResource};

/// Bytes loaded through the document's local and remote security policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadedResource {
    pub(crate) bytes: Vec<u8>,
    pub(crate) media_type: Option<String>,
}

/// Conversion-owned resource authority and cache.
#[derive(Debug, Default)]
pub(crate) struct ResourceLoader {
    resources: DocumentResources,
    cache: HashMap<ResolvedResource, LoadedResource>,
}

impl ResourceLoader {
    pub(crate) fn new(resources: DocumentResources) -> Self {
        Self {
            resources,
            cache: HashMap::new(),
        }
    }

    pub(crate) fn resources(&self) -> &DocumentResources {
        &self.resources
    }

    pub(crate) fn load(&mut self, reference: &str, base: Option<&Path>) -> Option<LoadedResource> {
        let resource = self.resources.resolve(reference, base)?;
        self.load_resolved(resource)
    }

    pub(crate) fn load_document_resource(&mut self, reference: &str) -> Option<LoadedResource> {
        let base = self.resources.base_path().map(Path::to_path_buf);
        self.load(reference, base.as_deref())
    }

    pub(crate) fn load_resolved(&mut self, resource: ResolvedResource) -> Option<LoadedResource> {
        if let Some(loaded) = self.cache.get(&resource) {
            return Some(loaded.clone());
        }
        let loaded = match &resource {
            ResolvedResource::Inline(uri) => load_data_uri(uri)?,
            ResolvedResource::Fragment(_) => return None,
            ResolvedResource::Local(path) => LoadedResource {
                bytes: std::fs::read(path.as_path()).ok()?,
                media_type: None,
            },
            #[cfg(feature = "remote")]
            ResolvedResource::Remote(url) => LoadedResource {
                bytes: crate::security::network::fetch_authorized(url, &self.resources.network)?,
                media_type: None,
            },
            #[cfg(not(feature = "remote"))]
            ResolvedResource::Remote(_) => return None,
        };
        self.cache.insert(resource, loaded.clone());
        Some(loaded)
    }
}

fn load_data_uri(uri: &str) -> Option<LoadedResource> {
    let rest = uri.get(5..)?;
    let (header, encoded) = rest.split_once(',')?;
    let header_lower = header.to_ascii_lowercase();
    let bytes = if header_lower
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        decode_base64(encoded)?
    } else {
        percent_decode(encoded).into_bytes()
    };
    let media_type = header_lower
        .split(';')
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Some(LoadedResource { bytes, media_type })
}

fn percent_decode(input: &str) -> String {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(pair) = bytes.get(index + 1..index + 3)
            && let (Some(high), Some(low)) = (hex_value(pair[0]), hex_value(pair[1]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
