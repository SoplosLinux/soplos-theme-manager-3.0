// Reads the XML that xfconf keeps for a channel and writes it back into a
// channel through xfconf-query. A theme carries the panel settings as that
// XML. Writing the file itself while xfconfd runs would be overwritten, and
// killing xfconfd is not something to do to a desktop, so the settings are
// set one by one through the daemon instead.

use std::process::Command;

#[derive(Debug, PartialEq)]
pub struct Setting {
    pub path: String,
    /// "string", "int", "bool"... "array" has several values.
    pub kind: String,
    pub values: Vec<(String, String)>,
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

// The attributes of a tag, in order.
fn attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].split_whitespace().last().unwrap_or("").to_string();
        let after = &rest[eq + 1..];
        let Some(open) = after.find('"') else { break };
        let Some(close) = after[open + 1..].find('"') else { break };
        out.push((name, unescape(&after[open + 1..open + 1 + close])));
        rest = &after[open + close + 2..];
    }
    out
}

fn attr<'a>(list: &'a [(String, String)], name: &str) -> Option<&'a str> {
    list.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// Every setting that holds a value. Containers ("empty") only give a path to
/// their children and are not returned.
pub fn parse(xml: &str) -> Result<Vec<Setting>, String> {
    let mut out: Vec<Setting> = Vec::new();
    // The names of the open properties, and the index of the array being read.
    let mut stack: Vec<String> = Vec::new();
    let mut open_array: Vec<Option<usize>> = Vec::new();
    let mut seen_channel = false;
    let mut rest = xml;
    while let Some(start) = rest.find('<') {
        let tail = &rest[start..];
        if tail.starts_with("<!--") {
            let end = tail.find("-->").ok_or("Unclosed comment")?;
            rest = &tail[end + 3..];
            continue;
        }
        // The end of the tag, ignoring a '>' inside quotes.
        let mut quoted = false;
        let mut end = None;
        for (i, c) in tail.char_indices() {
            match c {
                '"' => quoted = !quoted,
                '>' if !quoted => {
                    end = Some(i);
                    break;
                }
                _ => {}
            }
        }
        let end = end.ok_or("Unclosed tag")?;
        let tag = &tail[1..end];
        rest = &tail[end + 1..];
        if tag.starts_with('?') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if name.trim() == "property" {
                stack.pop();
                open_array.pop();
            }
            continue;
        }
        let self_closing = tag.ends_with('/');
        let body = tag.trim_end_matches('/');
        let name = body.split_whitespace().next().unwrap_or("");
        let attrs = attributes(body);
        match name {
            "channel" => seen_channel = true,
            "property" => {
                let key = attr(&attrs, "name").ok_or("A property without name")?.to_string();
                if key.is_empty() || key.contains('/') {
                    return Err("Not a valid property name".to_string());
                }
                let kind = attr(&attrs, "type").unwrap_or("empty").to_string();
                stack.push(key);
                let path = format!("/{}", stack.join("/"));
                let mut index = None;
                if kind == "array" {
                    out.push(Setting { path, kind, values: Vec::new() });
                    index = Some(out.len() - 1);
                } else if kind != "empty" {
                    let value = attr(&attrs, "value").unwrap_or("").to_string();
                    out.push(Setting { path, kind: kind.clone(), values: vec![(kind, value)] });
                }
                if self_closing {
                    stack.pop();
                } else {
                    open_array.push(index);
                }
            }
            "value" => {
                let at = open_array.last().copied().flatten().ok_or("A value outside an array")?;
                let kind = attr(&attrs, "type").unwrap_or("string").to_string();
                let value = attr(&attrs, "value").unwrap_or("").to_string();
                out[at].values.push((kind, value));
            }
            _ => {}
        }
    }
    if !seen_channel {
        return Err("It is not an xfconf file".to_string());
    }
    Ok(out)
}

const KINDS: &[&str] = &["string", "int", "uint", "bool", "double", "int64", "uint64"];

fn known(kind: &str) -> bool {
    KINDS.contains(&kind)
}

/// Replaces everything in the channel with the settings of the XML. The XML is
/// read completely first, so a broken file changes nothing. Returns how many
/// settings could not be written.
pub fn replace_channel(channel: &str, xml: &str) -> Result<usize, String> {
    let settings = parse(xml)?;
    if settings.is_empty() {
        return Err("The file has no settings".to_string());
    }
    if settings.iter().any(|s| !known(&s.kind) && s.kind != "array") {
        return Err("The file has a type of value that is not known".to_string());
    }
    let _ = Command::new("xfconf-query").args(["-c", channel, "-p", "/", "-r", "-R"]).output();
    let mut failed = 0;
    for s in &settings {
        let mut command = Command::new("xfconf-query");
        command.args(["-c", channel, "-p", &s.path, "--create"]);
        if s.kind == "array" {
            if s.values.is_empty() || s.values.iter().any(|(k, _)| !known(k)) {
                // An empty array cannot be made by xfconf-query; the panel does not need one.
                continue;
            }
            command.arg("--force-array");
            for (k, v) in &s.values {
                command.args(["-t", k, "-s", v]);
            }
        } else {
            command.args(["-t", &s.kind, "-s", &s.values[0].1]);
        }
        if !command.output().is_ok_and(|o| o.status.success()) {
            failed += 1;
        }
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<channel name="xfce4-panel" version="1.0">
  <property name="configver" type="int" value="2"/>
  <property name="panels" type="array">
    <value type="int" value="1"/>
    <property name="dark-mode" type="bool" value="true"/>
    <property name="panel-1" type="empty">
      <property name="position" type="string" value="p=8;x=0;y=0"/>
      <property name="plugin-ids" type="array">
        <value type="int" value="1"/>
        <value type="int" value="2"/>
      </property>
    </property>
  </property>
  <property name="plugins" type="empty">
    <property name="plugin-1" type="string" value="a &amp; b">
      <property name="x" type="uint" value="3"/>
    </property>
  </property>
</channel>"#;

    #[test]
    fn nested_properties_and_arrays_are_read() {
        let s = parse(XML).unwrap();
        let paths: Vec<&str> = s.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            ["/configver", "/panels", "/panels/dark-mode", "/panels/panel-1/position", "/panels/panel-1/plugin-ids", "/plugins/plugin-1", "/plugins/plugin-1/x"]
        );
        assert_eq!(s[1].values, vec![("int".to_string(), "1".to_string())]);
        assert_eq!(s[4].values.len(), 2);
        assert_eq!(s[5].values[0].1, "a & b");
    }

    #[test]
    fn a_file_that_is_not_xfconf_is_refused() {
        assert!(parse("<html></html>").is_err());
    }

    #[test]
    fn a_channel_is_replaced_and_read_back() {
        if Command::new("xfconf-query").args(["-c", "xsettings", "-l"]).output().is_err_and(|_| true) {
            return;
        }
        let channel = "soplos-tm-test";
        let failed = replace_channel(channel, XML).unwrap();
        assert_eq!(failed, 0);
        let got = Command::new("xfconf-query").args(["-c", channel, "-p", "/panels/panel-1/position"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&got.stdout).trim(), "p=8;x=0;y=0");
        let _ = Command::new("xfconf-query").args(["-c", channel, "-p", "/", "-r", "-R"]).output();
    }
}
