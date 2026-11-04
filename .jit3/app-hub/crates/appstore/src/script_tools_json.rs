//! Bounded JSON conversion at the untrusted VM -> host tool boundary.
//! Write while walking, rather than expanding an aliased graph into an
//! unbounded String before checking its size. Only the ancestor path counts
//! as a cycle, so small shared values still serialize normally.
use makepad_widgets::makepad_script::{ScriptHeap, ScriptValue};
use std::io::{self, Write};

struct Bounded {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Bounded {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("tool JSON exceeds 1 MiB"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::other(message)
}

fn string(heap: &ScriptHeap, value: ScriptValue, out: &mut Bounded) -> io::Result<()> {
    if let Some(id) = value.as_id() {
        return id.as_string(|text| match text {
            Some(text) => serde_json::to_writer(out, text).map_err(io::Error::other),
            None => Err(invalid("unknown JSON key")),
        });
    }
    heap.string_with(value, |_, text| {
        serde_json::to_writer(out, text).map_err(io::Error::other)
    })
    .unwrap_or_else(|| Err(invalid("JSON object keys must be strings")))
}

fn visit(
    heap: &ScriptHeap,
    value: ScriptValue,
    path: &mut Vec<ScriptValue>,
    out: &mut Bounded,
) -> io::Result<()> {
    if path.len() >= 32 || path.contains(&value) {
        return Err(invalid("cyclic or overly nested tool JSON"));
    }
    if value.as_object().is_some() {
        out.write_all(b"{")?;
        let base = path.len();
        let mut node = value;
        let mut first = true;
        while let Some(object) = node.as_object() {
            if path.len() >= 32 || path.contains(&node) {
                return Err(invalid("cyclic or overly nested tool JSON"));
            }
            path.push(node);
            let data = heap.object_data(object);
            for (key, entry) in data.map.iter() {
                if !first {
                    out.write_all(b",")?;
                }
                first = false;
                string(heap, *key, out)?;
                out.write_all(b":")?;
                visit(heap, entry.value, path, out)?;
            }
            for entry in &data.vec {
                if !first {
                    out.write_all(b",")?;
                }
                first = false;
                string(heap, entry.key, out)?;
                out.write_all(b":")?;
                visit(heap, entry.value, path, out)?;
            }
            node = data.proto;
        }
        path.truncate(base);
        out.write_all(b"}")
    } else if let Some(array) = value.as_array() {
        path.push(value);
        out.write_all(b"[")?;
        let values = heap.array_storage(array);
        for index in 0..values.len() {
            if index > 0 {
                out.write_all(b",")?;
            }
            visit(
                heap,
                values
                    .index(index)
                    .ok_or_else(|| invalid("invalid JSON array"))?,
                path,
                out,
            )?;
        }
        path.pop();
        out.write_all(b"]")
    } else if value.as_id().is_some()
        || value.as_string().is_some()
        || value.as_inline_string(|_| ()).is_some()
    {
        string(heap, value, out)
    } else if let Some(number) = value.as_number() {
        if !number.is_finite() {
            return Err(invalid("JSON number must be finite"));
        }
        // Match Splash's existing compact representation (13 stays 13, not
        // 13.0); formatting a finite f64 uses a bounded stack-sized value.
        write!(out, "{number}")
    } else if let Some(boolean) = value.as_bool() {
        out.write_all(if boolean { b"true" } else { b"false" })
    } else if value.is_nil() {
        out.write_all(b"null")
    } else {
        Err(invalid("expected JSON data"))
    }
}

pub(crate) fn result(
    heap: &ScriptHeap,
    value: ScriptValue,
    limit: usize,
) -> Result<serde_json::Value, String> {
    // A fixed-capacity buffer prevents geometric Vec growth beyond the cap.
    let mut out = Bounded {
        bytes: Vec::with_capacity(limit),
        limit,
    };
    visit(heap, value, &mut Vec::with_capacity(32), &mut out)
        .map_err(|error| format!("invalid_result: {error}"))?;
    serde_json::from_slice(&out.bytes).map_err(|_| "invalid_result: expected JSON data".into())
}
