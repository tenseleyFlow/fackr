//! Live smoke test against the wolf language server (`wolf lsp`).
//!
//! This drives fackr's *own* client — `LspClient` → `LspManager` →
//! `ServerProcess` — against a real server process, because the bugs this
//! module's patches fixed (framing on a character boundary, an undrained
//! stderr, a position encoding nobody negotiated) are all invisible to a unit
//! test that never speaks to anything.
//!
//! **It skips loudly when there is no server.** fackr spawns language servers
//! by bare `PATH` lookup, so the test needs a binary named `wolf` on `PATH`
//! (or `WOLF_BIN` pointing at one); with neither it prints `SKIP:` and
//! returns. A test that silently passes for doing nothing is a test nobody
//! notices is dark.
//!
//! The fixtures are inline on purpose: they are *this* test's subjects, and
//! the position they assert is the point. Both put an emoji — one code point,
//! two UTF-16 code units, four bytes — to the left of the thing being
//! located, so a column that is right under UTF-32 is wrong under every other
//! reading, and the assertions below fail rather than drift.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::client::{LspClient, LspResponse};
use super::types::PositionEncoding;

/// A file that fails to parse, with the offending token after astral text.
///
/// `E0002` (an empty statement) lands on the second `;` of line index 4:
///
/// ```text
///     let paw = "🐺" ; ;
/// //  0   4   8   ^12  ^18 ^20     <- code points
/// ```
///
/// Column 20 in code points is column 21 in UTF-16 units and 23 in bytes, so
/// the assertion on 20 is an assertion that the negotiation held.
const BROKEN: &str = "\
//! check: fail(E0002)
//! phase: parse

fn main() -> !int {
    let paw = \"\u{1f43a}\" ; ;
    0
}
";

/// A file that parses, with an identifier after astral text on its own line.
///
/// Hovering `paw` on line index 5 is the client→server direction: the client
/// counts 22 code points to the left of it, and only a server that agreed to
/// count the same way resolves the identifier rather than the space before it.
const CLEAN: &str = "\
//! check: run(exit=0)
//! phase: resolve

fn main() -> !int {
    let paw = \"\u{1f43a}\"
    let tail = \"\u{1f43a}\" != paw
    if tail { 0 } else { 1 }
}
";

/// Line/character of `paw` in [`CLEAN`], counted in code points.
const HOVER_AT: (u32, u32) = (5, 22);
/// Line/character of the `E0002` token in [`BROKEN`], counted in code points.
const DIAGNOSTIC_AT: (u32, u32) = (4, 20);

/// How long to wait on a cold server. fackr blocks its UI thread for up to
/// 5 s waiting for `Ready`, so anything slower than this is already a bug
/// the user would feel as a frozen editor.
const DEADLINE: Duration = Duration::from_secs(20);

/// Find a directory holding a binary fackr can spawn as bare `wolf`.
///
/// `WOLF_BIN` wins, then `PATH`. The file has to actually be *named* `wolf`:
/// fackr's `ServerConfig` spawns `Command::new("wolf")` with no override, and
/// pretending otherwise here would test a path the editor never takes.
fn wolf_bin_dir() -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { "wolf.exe" } else { "wolf" };

    if let Ok(bin) = std::env::var("WOLF_BIN") {
        let path = PathBuf::from(&bin);
        if !path.is_file() {
            return Err(format!("WOLF_BIN={bin} is not a file"));
        }
        if path.file_name().map(|n| n != exe).unwrap_or(true) {
            return Err(format!(
                "WOLF_BIN={bin} is not named `{exe}` — fackr spawns the bare name, \
                 so point WOLF_BIN at a file with that name"
            ));
        }
        return Ok(path.parent().unwrap_or(Path::new(".")).to_path_buf());
    }

    let path_var = std::env::var_os("PATH").ok_or_else(|| "no PATH".to_string())?;
    for dir in std::env::split_paths(&path_var) {
        if dir.join(exe).is_file() {
            return Ok(dir);
        }
    }
    Err(format!("no `{exe}` on PATH and no WOLF_BIN"))
}

/// A scratch workspace holding the two fixtures, removed on drop.
struct Workspace {
    dir: PathBuf,
}

impl Workspace {
    fn new() -> std::io::Result<Self> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("fackr-wolf-smoke-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("broken.lu"), BROKEN)?;
        std::fs::write(dir.join("clean.lu"), CLEAN)?;
        Ok(Self { dir })
    }

    fn path(&self, name: &str) -> String {
        self.dir.join(name).to_string_lossy().into_owned()
    }

    fn root(&self) -> String {
        self.dir.to_string_lossy().into_owned()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Pump the client until `done` is satisfied, or give up with the server's
/// own account of itself — which is the reason the log exists.
fn pump<T>(
    client: &mut LspClient,
    what: &str,
    mut done: impl FnMut(&mut LspClient) -> Option<T>,
) -> T {
    let deadline = Instant::now() + DEADLINE;
    loop {
        client.process_messages();
        if let Some(value) = done(client) {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {what}; server said: {:?}",
            client.server_log()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// initialize → didOpen → diagnostics → hover, against a real `wolf lsp`.
#[test]
fn wolf_lsp_session() {
    let bin_dir = match wolf_bin_dir() {
        Ok(dir) => dir,
        Err(why) => {
            println!("SKIP: {why}");
            return;
        }
    };

    // fackr resolves servers through PATH and nothing else, so this is how a
    // test points it at a particular build. No other test in this crate
    // spawns a process, so the mutation is contained.
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs = vec![bin_dir];
    dirs.extend(std::env::split_paths(&path_var));
    std::env::set_var("PATH", std::env::join_paths(dirs).expect("joinable PATH"));

    let ws = Workspace::new().expect("scratch workspace");
    let broken = ws.path("broken.lu");
    let clean = ws.path("clean.lu");
    let mut client = LspClient::new(&ws.root());

    // Registration: `.lu` maps to a languageId with a configured server, so
    // the didOpen below starts one instead of being dropped on the floor.
    // (`has_server_for_file` is about a *running* server, so it only turns
    // true after the handshake below.)
    assert_eq!(super::types::detect_language(&broken), Some("wolf"));

    client.open_document(&broken, BROKEN).expect("didOpen");

    let diagnostics = pump(&mut client, "diagnostics on broken.lu", |c| {
        let diags = c.get_diagnostics(&broken);
        (!diags.is_empty()).then_some(diags)
    });

    // The negotiation is what makes every column below meaningful.
    assert_eq!(
        client.position_encoding("wolf"),
        Some(PositionEncoding::Utf32),
        "the server did not accept utf-32; every column in this session is then \
         off by one per preceding astral character"
    );

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    let diagnostic = &diagnostics[0];
    assert_eq!(diagnostic.code.as_deref(), Some("E0002"));
    assert_eq!(
        (
            diagnostic.range.start.line,
            diagnostic.range.start.character
        ),
        DIAGNOSTIC_AT,
        "the diagnostic landed on the wrong character — 21 would mean UTF-16, \
         23 would mean bytes"
    );
    // D22: the message reaches the editor as the compiler wrote it, em dash
    // and all.
    assert!(
        diagnostic
            .message
            .starts_with("this `;` terminates nothing"),
        "message: {:?}",
        diagnostic.message
    );

    // The other direction: a position this client counted in code points.
    client.open_document(&clean, CLEAN).expect("didOpen clean");
    let id = client
        .request_hover(&clean, HOVER_AT.0, HOVER_AT.1)
        .expect("hover request");

    let hover = pump(&mut client, "hover response", |c| match c.poll_response() {
        Some(LspResponse::Hover(rid, hover)) if rid == id => Some(hover),
        Some(LspResponse::Error(rid, e)) if rid == id => panic!("hover failed: {e}"),
        _ => None,
    })
    .expect("hover resolved a token");

    assert!(
        hover.contents.contains("str"),
        "hover at the identifier after the emoji resolved something else: {:?}",
        hover.contents
    );
    let range = hover.range.expect("hover carries the token's range");
    assert_eq!(
        (range.start.line, range.start.character),
        HOVER_AT,
        "the server resolved a different token — one code point left is the space"
    );
    assert_eq!(
        range.end.character,
        HOVER_AT.1 + 3,
        "`paw` is three columns wide"
    );

    // Full-text sync: fackr never sends an incremental change, and the server
    // advertises `textDocumentSync.change: 1` (Full), so this is the only
    // shape of edit it will ever see from this client.
    let edited = CLEAN.replace("if tail { 0 } else { 1 }", "if tail { 0 } else { 1 } ; ;");
    client
        .document_changed(&clean, &edited)
        .expect("full-text didChange");
    let after_edit = pump(&mut client, "diagnostics after didChange", |c| {
        let diags = c.get_diagnostics(&clean);
        (!diags.is_empty()).then_some(diags)
    });
    assert_eq!(after_edit[0].code.as_deref(), Some("E0002"));

    // …and back: the same document, full-text, returns to clean.
    client
        .document_changed(&clean, CLEAN)
        .expect("full-text didChange back");
    pump(&mut client, "diagnostics cleared after revert", |c| {
        c.get_diagnostics(&clean).is_empty().then_some(())
    });

    client.shutdown();
}

/// The session wolf-lsp records as `transcripts/fackr/smoke.jsonl`.
///
/// It runs against the **vendored corpus** rather than the inline fixtures
/// above, because the transcript is committed in a repo where those samples
/// are the only legal `.lu` and a replay has to find the files this session
/// opened. Point `FACKR_SMOKE_CORPUS` at a directory holding `hello.lu` and
/// `grammar/semicolon.lu` (wolf-lsp's `vendor/upstream/samples`) and run the
/// editor with a capture proxy first on `PATH`; with the variable unset this
/// skips, because there is nothing honest to run against.
#[test]
fn wolf_lsp_corpus_session() {
    let Ok(corpus) = std::env::var("FACKR_SMOKE_CORPUS") else {
        println!("SKIP: FACKR_SMOKE_CORPUS is unset (the recorded-session subject)");
        return;
    };
    let corpus = PathBuf::from(corpus);
    let hello = corpus.join("hello.lu");
    let broken = corpus.join("grammar").join("semicolon.lu");
    if !hello.is_file() || !broken.is_file() {
        println!("SKIP: {} is not a wolf corpus", corpus.display());
        return;
    }
    if let Ok(dir) = wolf_bin_dir() {
        let path_var = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs = vec![dir];
        dirs.extend(std::env::split_paths(&path_var));
        std::env::set_var("PATH", std::env::join_paths(dirs).expect("joinable PATH"));
    } else {
        println!("SKIP: no wolf binary");
        return;
    }

    let hello_path = hello.to_string_lossy().into_owned();
    let broken_path = broken.to_string_lossy().into_owned();
    let hello_text = std::fs::read_to_string(&hello).expect("hello.lu");
    let broken_text = std::fs::read_to_string(&broken).expect("semicolon.lu");

    let mut client = LspClient::new(&corpus.to_string_lossy());

    // A clean file first: open, diagnostics (an empty publish), hover on the
    // `who` binding, document symbols.
    client
        .open_document(&hello_path, &hello_text)
        .expect("didOpen hello.lu");
    pump(&mut client, "publish for hello.lu", |c| {
        c.get_all_diagnostics()
            .contains_key(&super::types::path_to_uri(&hello_path))
            .then_some(())
    });

    // `who` inside the interpolation on `print("hello, {who}")`.
    let hover_id = client
        .request_hover(&hello_path, 10, 20)
        .expect("hover request");
    pump(&mut client, "hover on hello.lu", |c| {
        match c.poll_response() {
            Some(LspResponse::Hover(id, _)) if id == hover_id => Some(()),
            _ => None,
        }
    });

    let symbols_id = client
        .request_document_symbols(&hello_path)
        .expect("documentSymbol request");
    pump(&mut client, "documentSymbol", |c| match c.poll_response() {
        Some(LspResponse::Symbols(id, _)) if id == symbols_id => Some(()),
        _ => None,
    });

    let format_id = client
        .request_formatting(&hello_path, 4, true)
        .expect("formatting request");
    pump(&mut client, "formatting", |c| match c.poll_response() {
        Some(LspResponse::Formatting(id, _)) if id == format_id => Some(()),
        _ => None,
    });

    // Then a file with a pinned diagnostic, and a full-text edit over it —
    // the only shape of change this client ever sends.
    client
        .open_document(&broken_path, &broken_text)
        .expect("didOpen semicolon.lu");
    let diagnostics = pump(&mut client, "diagnostics on semicolon.lu", |c| {
        let d = c.get_diagnostics(&broken_path);
        (!d.is_empty()).then_some(d)
    });
    assert_eq!(diagnostics[0].code.as_deref(), Some("E0002"));

    client
        // Delete the stray `;` — the whole buffer, as always.
        .document_changed(&broken_path, &broken_text.replacen("    ;", "     ", 1))
        .expect("full-text didChange");
    pump(&mut client, "diagnostics cleared by the edit", |c| {
        c.get_diagnostics(&broken_path).is_empty().then_some(())
    });

    client.close_document(&broken_path).expect("didClose");
    client.shutdown();
}
