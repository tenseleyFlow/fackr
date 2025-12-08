//! Language definitions for syntax highlighting

#![allow(dead_code)]

use std::collections::HashSet;

/// Supported programming languages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    Go,
    Java,
    Kotlin,
    Swift,
    Ruby,
    Php,
    CSharp,
    Scala,
    Haskell,
    Lua,
    Perl,
    R,
    Julia,
    Elixir,
    Erlang,
    Clojure,
    Fortran,
    Zig,
    Nim,
    Odin,
    V,
    D,
    Bash,
    Fish,
    Zsh,
    PowerShell,
    Sql,
    Html,
    Css,
    Json,
    Yaml,
    Toml,
    Xml,
    Markdown,
    Makefile,
    Dockerfile,
    Terraform,
    Nix,
    Ocaml,
    Fsharp,
    Dart,
    Groovy,
}

impl Language {
    /// Detect language from filename/extension
    pub fn detect(filename: &str) -> Option<Language> {
        let lower = filename.to_lowercase();

        // Check full filename first (for files like Makefile, Dockerfile)
        let basename = lower.rsplit('/').next().unwrap_or(&lower);

        match basename {
            "makefile" | "gnumakefile" => return Some(Language::Makefile),
            "dockerfile" => return Some(Language::Dockerfile),
            "cmakelists.txt" => return Some(Language::Makefile),
            ".bashrc" | ".bash_profile" | ".profile" => return Some(Language::Bash),
            ".zshrc" | ".zprofile" => return Some(Language::Zsh),
            "cargo.toml" | "pyproject.toml" => return Some(Language::Toml),
            "package.json" | "tsconfig.json" => return Some(Language::Json),
            _ => {}
        }

        // Check extension
        let ext = lower.rsplit('.').next()?;

        match ext {
            // Rust
            "rs" => Some(Language::Rust),

            // Python
            "py" | "pyw" | "pyi" | "pyx" => Some(Language::Python),

            // JavaScript / TypeScript
            "js" | "mjs" | "cjs" | "jsx" => Some(Language::JavaScript),
            "ts" | "mts" | "cts" | "tsx" => Some(Language::TypeScript),

            // C / C++
            "c" | "h" => Some(Language::C),
            "cpp" | "cc" | "cxx" | "c++" | "hpp" | "hh" | "hxx" | "h++" | "ipp" => Some(Language::Cpp),

            // Go
            "go" => Some(Language::Go),

            // Java / JVM languages
            "java" => Some(Language::Java),
            "kt" | "kts" => Some(Language::Kotlin),
            "scala" | "sc" => Some(Language::Scala),
            "groovy" | "gvy" | "gy" | "gsh" => Some(Language::Groovy),
            "clj" | "cljs" | "cljc" | "edn" => Some(Language::Clojure),

            // Apple / Swift
            "swift" => Some(Language::Swift),

            // Ruby
            "rb" | "rake" | "gemspec" => Some(Language::Ruby),

            // PHP
            "php" | "php3" | "php4" | "php5" | "phtml" => Some(Language::Php),

            // C# / .NET
            "cs" => Some(Language::CSharp),
            "fs" | "fsx" | "fsi" => Some(Language::Fsharp),

            // Functional languages
            "hs" | "lhs" => Some(Language::Haskell),
            "ml" | "mli" => Some(Language::Ocaml),
            "ex" | "exs" => Some(Language::Elixir),
            "erl" | "hrl" => Some(Language::Erlang),

            // Scripting
            "lua" => Some(Language::Lua),
            "pl" | "pm" | "t" => Some(Language::Perl),
            "r" | "R" => Some(Language::R),
            "jl" => Some(Language::Julia),

            // System languages
            "zig" => Some(Language::Zig),
            "nim" | "nims" => Some(Language::Nim),
            "odin" => Some(Language::Odin),
            "v" => Some(Language::V),
            "d" => Some(Language::D),
            "f90" | "f95" | "f03" | "f08" | "f18" | "f" | "for" => Some(Language::Fortran),

            // Shell
            "sh" | "bash" => Some(Language::Bash),
            "fish" => Some(Language::Fish),
            "zsh" => Some(Language::Zsh),
            "ps1" | "psm1" | "psd1" => Some(Language::PowerShell),

            // Data / Config
            "sql" => Some(Language::Sql),
            "json" | "jsonc" | "json5" => Some(Language::Json),
            "yaml" | "yml" => Some(Language::Yaml),
            "toml" => Some(Language::Toml),
            "xml" | "svg" | "xsl" | "xslt" => Some(Language::Xml),

            // Web
            "html" | "htm" | "xhtml" => Some(Language::Html),
            "css" | "scss" | "sass" | "less" => Some(Language::Css),

            // Documentation
            "md" | "markdown" | "mdown" | "mkd" => Some(Language::Markdown),

            // DevOps / Infrastructure
            "tf" | "tfvars" => Some(Language::Terraform),
            "nix" => Some(Language::Nix),

            // Flutter / Dart
            "dart" => Some(Language::Dart),

            _ => None,
        }
    }

    /// Get the language definition
    pub fn definition(&self) -> LanguageDef {
        match self {
            Language::Rust => rust_def(),
            Language::Python => python_def(),
            Language::JavaScript => javascript_def(),
            Language::TypeScript => typescript_def(),
            Language::C => c_def(),
            Language::Cpp => cpp_def(),
            Language::Go => go_def(),
            Language::Java => java_def(),
            Language::Kotlin => kotlin_def(),
            Language::Swift => swift_def(),
            Language::Ruby => ruby_def(),
            Language::Php => php_def(),
            Language::CSharp => csharp_def(),
            Language::Scala => scala_def(),
            Language::Haskell => haskell_def(),
            Language::Lua => lua_def(),
            Language::Perl => perl_def(),
            Language::R => r_def(),
            Language::Julia => julia_def(),
            Language::Elixir => elixir_def(),
            Language::Erlang => erlang_def(),
            Language::Clojure => clojure_def(),
            Language::Fortran => fortran_def(),
            Language::Zig => zig_def(),
            Language::Nim => nim_def(),
            Language::Odin => odin_def(),
            Language::V => v_def(),
            Language::D => d_def(),
            Language::Bash => bash_def(),
            Language::Fish => fish_def(),
            Language::Zsh => zsh_def(),
            Language::PowerShell => powershell_def(),
            Language::Sql => sql_def(),
            Language::Html => html_def(),
            Language::Css => css_def(),
            Language::Json => json_def(),
            Language::Yaml => yaml_def(),
            Language::Toml => toml_def(),
            Language::Xml => xml_def(),
            Language::Markdown => markdown_def(),
            Language::Makefile => makefile_def(),
            Language::Dockerfile => dockerfile_def(),
            Language::Terraform => terraform_def(),
            Language::Nix => nix_def(),
            Language::Ocaml => ocaml_def(),
            Language::Fsharp => fsharp_def(),
            Language::Dart => dart_def(),
            Language::Groovy => groovy_def(),
        }
    }

    /// Get display name
    pub fn name(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::Go => "Go",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::Swift => "Swift",
            Language::Ruby => "Ruby",
            Language::Php => "PHP",
            Language::CSharp => "C#",
            Language::Scala => "Scala",
            Language::Haskell => "Haskell",
            Language::Lua => "Lua",
            Language::Perl => "Perl",
            Language::R => "R",
            Language::Julia => "Julia",
            Language::Elixir => "Elixir",
            Language::Erlang => "Erlang",
            Language::Clojure => "Clojure",
            Language::Fortran => "Fortran",
            Language::Zig => "Zig",
            Language::Nim => "Nim",
            Language::Odin => "Odin",
            Language::V => "V",
            Language::D => "D",
            Language::Bash => "Bash",
            Language::Fish => "Fish",
            Language::Zsh => "Zsh",
            Language::PowerShell => "PowerShell",
            Language::Sql => "SQL",
            Language::Html => "HTML",
            Language::Css => "CSS",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Xml => "XML",
            Language::Markdown => "Markdown",
            Language::Makefile => "Makefile",
            Language::Dockerfile => "Dockerfile",
            Language::Terraform => "Terraform",
            Language::Nix => "Nix",
            Language::Ocaml => "OCaml",
            Language::Fsharp => "F#",
            Language::Dart => "Dart",
            Language::Groovy => "Groovy",
        }
    }
}

/// Language definition for syntax highlighting
#[derive(Debug, Clone)]
pub struct LanguageDef {
    pub name: &'static str,
    pub keywords: HashSet<&'static str>,
    pub types: HashSet<&'static str>,
    pub line_comment: Option<&'static str>,
    pub block_comment_start: Option<&'static str>,
    pub block_comment_end: Option<&'static str>,
    pub string_delimiters: Vec<char>,
    pub multiline_strings: bool,
    pub operators: Vec<&'static str>,
    pub punctuation: Vec<char>,
    pub has_preprocessor: bool,
    pub case_sensitive: bool,
}

impl Default for LanguageDef {
    fn default() -> Self {
        Self {
            name: "Plain",
            keywords: HashSet::new(),
            types: HashSet::new(),
            line_comment: None,
            block_comment_start: None,
            block_comment_end: None,
            string_delimiters: vec!['"', '\''],
            multiline_strings: false,
            operators: vec![],
            punctuation: vec![],
            has_preprocessor: false,
            case_sensitive: true,
        }
    }
}

// Common operators used by C-like languages
const C_OPERATORS: &[&str] = &[
    "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||",
    "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
    "+", "-", "*", "/", "%", "&", "|", "^", "~", "!", "<", ">", "=", "?", ":",
];

const C_PUNCTUATION: &[char] = &['{', '}', '(', ')', '[', ']', ';', ',', '.'];

// ============================================================================
// Language Definitions
// ============================================================================

fn rust_def() -> LanguageDef {
    LanguageDef {
        name: "Rust",
        keywords: [
            "as", "async", "await", "break", "const", "continue", "crate", "dyn",
            "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in",
            "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
            "self", "Self", "static", "struct", "super", "trait", "true", "type",
            "unsafe", "use", "where", "while", "yield",
        ].into_iter().collect(),
        types: [
            "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize",
            "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64",
            "String", "Vec", "Box", "Rc", "Arc", "Cell", "RefCell", "Option",
            "Result", "Ok", "Err", "Some", "None", "HashMap", "HashSet",
            "BTreeMap", "BTreeSet", "VecDeque", "LinkedList", "BinaryHeap",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"'],
        multiline_strings: false,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn python_def() -> LanguageDef {
    LanguageDef {
        name: "Python",
        keywords: [
            "False", "None", "True", "and", "as", "assert", "async", "await",
            "break", "class", "continue", "def", "del", "elif", "else", "except",
            "finally", "for", "from", "global", "if", "import", "in", "is",
            "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try",
            "while", "with", "yield", "match", "case", "type",
        ].into_iter().collect(),
        types: [
            "int", "float", "str", "bool", "list", "dict", "set", "tuple",
            "bytes", "bytearray", "complex", "frozenset", "object", "type",
            "None", "Callable", "Iterator", "Generator", "Coroutine",
            "Optional", "Union", "Any", "List", "Dict", "Set", "Tuple",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: true, // """...""" and '''...'''
        operators: vec![
            "**", "//", "<<", ">>", "<=", ">=", "==", "!=", "->",
            "+=", "-=", "*=", "/=", "//=", "%=", "**=", "&=", "|=", "^=", ">>=", "<<=",
            "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">", "=", "@",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ':', ',', '.', ';'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn javascript_def() -> LanguageDef {
    LanguageDef {
        name: "JavaScript",
        keywords: [
            "async", "await", "break", "case", "catch", "class", "const",
            "continue", "debugger", "default", "delete", "do", "else", "export",
            "extends", "false", "finally", "for", "function", "if", "import",
            "in", "instanceof", "let", "new", "null", "of", "return", "static",
            "super", "switch", "this", "throw", "true", "try", "typeof", "var",
            "void", "while", "with", "yield", "undefined", "NaN", "Infinity",
        ].into_iter().collect(),
        types: [
            "Array", "Boolean", "Date", "Error", "Function", "JSON", "Map",
            "Math", "Number", "Object", "Promise", "Proxy", "RegExp", "Set",
            "String", "Symbol", "WeakMap", "WeakSet", "BigInt", "ArrayBuffer",
            "DataView", "Float32Array", "Float64Array", "Int8Array", "Int16Array",
            "Int32Array", "Uint8Array", "Uint16Array", "Uint32Array",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true, // template literals
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn typescript_def() -> LanguageDef {
    let mut def = javascript_def();
    def.name = "TypeScript";
    def.keywords.extend([
        "abstract", "as", "asserts", "declare", "enum", "get", "implements",
        "interface", "is", "keyof", "module", "namespace", "never", "override",
        "private", "protected", "public", "readonly", "require", "set", "type",
        "infer", "satisfies",
    ]);
    def.types.extend([
        "any", "boolean", "never", "null", "number", "object", "string",
        "symbol", "undefined", "unknown", "void", "Partial", "Required",
        "Readonly", "Record", "Pick", "Omit", "Exclude", "Extract",
        "NonNullable", "Parameters", "ReturnType", "InstanceType",
    ]);
    def
}

fn c_def() -> LanguageDef {
    LanguageDef {
        name: "C",
        keywords: [
            "auto", "break", "case", "const", "continue", "default", "do",
            "else", "enum", "extern", "for", "goto", "if", "inline", "register",
            "restrict", "return", "sizeof", "static", "struct", "switch",
            "typedef", "union", "volatile", "while", "_Alignas", "_Alignof",
            "_Atomic", "_Bool", "_Complex", "_Generic", "_Imaginary",
            "_Noreturn", "_Static_assert", "_Thread_local",
        ].into_iter().collect(),
        types: [
            "char", "double", "float", "int", "long", "short", "signed",
            "unsigned", "void", "size_t", "ssize_t", "ptrdiff_t", "intptr_t",
            "uintptr_t", "int8_t", "int16_t", "int32_t", "int64_t",
            "uint8_t", "uint16_t", "uint32_t", "uint64_t", "bool", "FILE",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: true,
        case_sensitive: true,
    }
}

fn cpp_def() -> LanguageDef {
    let mut def = c_def();
    def.name = "C++";
    def.keywords.extend([
        "alignas", "alignof", "and", "and_eq", "asm", "bitand", "bitor",
        "catch", "class", "compl", "concept", "consteval", "constexpr",
        "constinit", "const_cast", "co_await", "co_return", "co_yield",
        "decltype", "delete", "dynamic_cast", "explicit", "export", "false",
        "friend", "module", "mutable", "namespace", "new", "noexcept", "not",
        "not_eq", "nullptr", "operator", "or", "or_eq", "private", "protected",
        "public", "reinterpret_cast", "requires", "static_assert", "static_cast",
        "template", "this", "throw", "true", "try", "typeid", "typename",
        "using", "virtual", "xor", "xor_eq", "override", "final",
    ]);
    def.types.extend([
        "auto", "wchar_t", "char8_t", "char16_t", "char32_t", "string",
        "wstring", "string_view", "vector", "map", "unordered_map", "set",
        "unordered_set", "list", "deque", "array", "pair", "tuple",
        "optional", "variant", "any", "span", "unique_ptr", "shared_ptr",
        "weak_ptr", "function", "thread", "mutex", "atomic",
    ]);
    def
}

fn go_def() -> LanguageDef {
    LanguageDef {
        name: "Go",
        keywords: [
            "break", "case", "chan", "const", "continue", "default", "defer",
            "else", "fallthrough", "for", "func", "go", "goto", "if", "import",
            "interface", "map", "package", "range", "return", "select", "struct",
            "switch", "type", "var", "true", "false", "nil", "iota",
        ].into_iter().collect(),
        types: [
            "bool", "byte", "complex64", "complex128", "error", "float32",
            "float64", "int", "int8", "int16", "int32", "int64", "rune",
            "string", "uint", "uint8", "uint16", "uint32", "uint64", "uintptr",
            "any", "comparable",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true,
        operators: vec![
            ":=", "...", "++", "--", "<<", ">>", "&^", "<=", ">=", "==", "!=",
            "&&", "||", "<-", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=",
            "<<=", ">>=", "&^=",
            "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "=", "!",
        ],
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn java_def() -> LanguageDef {
    LanguageDef {
        name: "Java",
        keywords: [
            "abstract", "assert", "boolean", "break", "byte", "case", "catch",
            "char", "class", "const", "continue", "default", "do", "double",
            "else", "enum", "extends", "final", "finally", "float", "for",
            "goto", "if", "implements", "import", "instanceof", "int",
            "interface", "long", "native", "new", "package", "private",
            "protected", "public", "return", "short", "static", "strictfp",
            "super", "switch", "synchronized", "this", "throw", "throws",
            "transient", "try", "void", "volatile", "while", "true", "false",
            "null", "var", "yield", "record", "sealed", "non-sealed", "permits",
        ].into_iter().collect(),
        types: [
            "Boolean", "Byte", "Character", "Class", "Double", "Enum", "Float",
            "Integer", "Long", "Number", "Object", "Short", "String", "Void",
            "List", "Map", "Set", "Collection", "ArrayList", "HashMap", "HashSet",
            "LinkedList", "TreeMap", "TreeSet", "Optional", "Stream", "Thread",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true, // text blocks with """
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn kotlin_def() -> LanguageDef {
    LanguageDef {
        name: "Kotlin",
        keywords: [
            "as", "break", "by", "catch", "class", "companion", "const",
            "constructor", "continue", "crossinline", "data", "do", "else",
            "enum", "external", "false", "final", "finally", "for", "fun",
            "get", "if", "import", "in", "infix", "init", "inline", "inner",
            "interface", "internal", "is", "lateinit", "noinline", "null",
            "object", "open", "operator", "out", "override", "package",
            "private", "protected", "public", "reified", "return", "sealed",
            "set", "super", "suspend", "tailrec", "this", "throw", "true",
            "try", "typealias", "typeof", "val", "var", "vararg", "when",
            "where", "while",
        ].into_iter().collect(),
        types: [
            "Any", "Boolean", "Byte", "Char", "Double", "Float", "Int", "Long",
            "Nothing", "Number", "Short", "String", "Unit", "Array", "List",
            "Map", "Set", "MutableList", "MutableMap", "MutableSet", "Pair",
            "Triple", "Sequence", "Lazy",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn swift_def() -> LanguageDef {
    LanguageDef {
        name: "Swift",
        keywords: [
            "actor", "any", "as", "associatedtype", "async", "await", "break",
            "case", "catch", "class", "continue", "convenience", "default",
            "defer", "deinit", "didSet", "do", "dynamic", "else", "enum",
            "extension", "fallthrough", "false", "fileprivate", "final", "for",
            "func", "get", "guard", "if", "import", "in", "indirect", "infix",
            "init", "inout", "internal", "is", "isolated", "lazy", "let",
            "mutating", "nil", "nonisolated", "nonmutating", "open", "operator",
            "optional", "override", "postfix", "precedencegroup", "prefix",
            "private", "protocol", "public", "repeat", "required", "rethrows",
            "return", "self", "Self", "set", "some", "static", "struct",
            "subscript", "super", "switch", "throw", "throws", "true", "try",
            "typealias", "unowned", "var", "weak", "where", "while", "willSet",
        ].into_iter().collect(),
        types: [
            "Any", "AnyObject", "Array", "Bool", "Character", "Dictionary",
            "Double", "Float", "Int", "Int8", "Int16", "Int32", "Int64",
            "Optional", "Result", "Set", "String", "UInt", "UInt8", "UInt16",
            "UInt32", "UInt64", "Void", "Never",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"'],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn ruby_def() -> LanguageDef {
    LanguageDef {
        name: "Ruby",
        keywords: [
            "BEGIN", "END", "alias", "and", "begin", "break", "case", "class",
            "def", "defined?", "do", "else", "elsif", "end", "ensure", "false",
            "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
            "rescue", "retry", "return", "self", "super", "then", "true",
            "undef", "unless", "until", "when", "while", "yield", "__FILE__",
            "__LINE__", "__ENCODING__", "lambda", "proc", "raise", "require",
            "require_relative", "attr_accessor", "attr_reader", "attr_writer",
            "private", "protected", "public",
        ].into_iter().collect(),
        types: [
            "Array", "Bignum", "Binding", "Class", "Continuation", "Dir",
            "Exception", "FalseClass", "File", "Fixnum", "Float", "Hash",
            "Integer", "IO", "MatchData", "Method", "Module", "NilClass",
            "Numeric", "Object", "Proc", "Range", "Regexp", "String", "Struct",
            "Symbol", "Thread", "Time", "TrueClass",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: Some("=begin"),
        block_comment_end: Some("=end"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true,
        operators: vec![
            "**", "..", "...", "<<", ">>", "<=>", "<=", ">=", "==", "===",
            "!=", "=~", "!~", "&&", "||", "+=", "-=", "*=", "/=", "%=",
            "**=", "&=", "|=", "^=", "<<=", ">>=", "&&=", "||=",
            "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">", "=", "!",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':', '@', '$'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn php_def() -> LanguageDef {
    LanguageDef {
        name: "PHP",
        keywords: [
            "abstract", "and", "array", "as", "break", "callable", "case",
            "catch", "class", "clone", "const", "continue", "declare", "default",
            "die", "do", "echo", "else", "elseif", "empty", "enddeclare",
            "endfor", "endforeach", "endif", "endswitch", "endwhile", "eval",
            "exit", "extends", "final", "finally", "fn", "for", "foreach",
            "function", "global", "goto", "if", "implements", "include",
            "include_once", "instanceof", "insteadof", "interface", "isset",
            "list", "match", "namespace", "new", "or", "print", "private",
            "protected", "public", "readonly", "require", "require_once",
            "return", "static", "switch", "throw", "trait", "try", "unset",
            "use", "var", "while", "xor", "yield", "true", "false", "null",
        ].into_iter().collect(),
        types: [
            "bool", "boolean", "int", "integer", "float", "double", "string",
            "array", "object", "callable", "iterable", "void", "mixed", "never",
            "null", "self", "parent", "static",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':', '$', '@'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn csharp_def() -> LanguageDef {
    LanguageDef {
        name: "C#",
        keywords: [
            "abstract", "as", "base", "bool", "break", "byte", "case", "catch",
            "char", "checked", "class", "const", "continue", "decimal", "default",
            "delegate", "do", "double", "else", "enum", "event", "explicit",
            "extern", "false", "finally", "fixed", "float", "for", "foreach",
            "goto", "if", "implicit", "in", "int", "interface", "internal",
            "is", "lock", "long", "namespace", "new", "null", "object",
            "operator", "out", "override", "params", "private", "protected",
            "public", "readonly", "ref", "return", "sbyte", "sealed", "short",
            "sizeof", "stackalloc", "static", "string", "struct", "switch",
            "this", "throw", "true", "try", "typeof", "uint", "ulong",
            "unchecked", "unsafe", "ushort", "using", "var", "virtual", "void",
            "volatile", "while", "async", "await", "dynamic", "nameof", "when",
            "record", "init", "with", "required", "file", "scoped",
        ].into_iter().collect(),
        types: [
            "Boolean", "Byte", "Char", "DateTime", "Decimal", "Double", "Guid",
            "Int16", "Int32", "Int64", "Object", "SByte", "Single", "String",
            "TimeSpan", "UInt16", "UInt32", "UInt64", "List", "Dictionary",
            "HashSet", "Queue", "Stack", "Array", "Task", "Func", "Action",
            "IEnumerable", "IList", "IDictionary", "ICollection",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: true,
        case_sensitive: true,
    }
}

fn scala_def() -> LanguageDef {
    LanguageDef {
        name: "Scala",
        keywords: [
            "abstract", "case", "catch", "class", "def", "do", "else", "enum",
            "export", "extends", "extension", "false", "final", "finally",
            "for", "forSome", "given", "if", "implicit", "import", "inline",
            "lazy", "match", "new", "null", "object", "opaque", "open",
            "override", "package", "private", "protected", "return", "sealed",
            "super", "then", "this", "throw", "trait", "transparent", "true",
            "try", "type", "using", "val", "var", "while", "with", "yield",
        ].into_iter().collect(),
        types: [
            "Any", "AnyRef", "AnyVal", "Array", "BigDecimal", "BigInt", "Boolean",
            "Byte", "Char", "Double", "Float", "Int", "List", "Long", "Map",
            "Nothing", "Null", "Option", "Seq", "Set", "Short", "Some", "String",
            "Unit", "Vector",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn haskell_def() -> LanguageDef {
    LanguageDef {
        name: "Haskell",
        keywords: [
            "as", "case", "class", "data", "default", "deriving", "do", "else",
            "forall", "foreign", "hiding", "if", "import", "in", "infix",
            "infixl", "infixr", "instance", "let", "mdo", "module", "newtype",
            "of", "proc", "qualified", "rec", "then", "type", "where",
        ].into_iter().collect(),
        types: [
            "Bool", "Char", "Double", "Either", "Float", "IO", "Int", "Integer",
            "Maybe", "Ordering", "String", "Word",
        ].into_iter().collect(),
        line_comment: Some("--"),
        block_comment_start: Some("{-"),
        block_comment_end: Some("-}"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "->", "<-", "=>", "::", "++", ">>", ">>=", "<$>", "<*>", "<|>",
            ".", "$", "=", "<", ">", "+", "-", "*", "/", "^", "&", "|",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '`'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn lua_def() -> LanguageDef {
    LanguageDef {
        name: "Lua",
        keywords: [
            "and", "break", "do", "else", "elseif", "end", "false", "for",
            "function", "goto", "if", "in", "local", "nil", "not", "or",
            "repeat", "return", "then", "true", "until", "while",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("--"),
        block_comment_start: Some("--[["),
        block_comment_end: Some("]]"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "..", "...", "==", "~=", "<=", ">=", "<<", ">>", "//",
            "+", "-", "*", "/", "%", "^", "#", "&", "|", "~", "<", ">", "=",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn perl_def() -> LanguageDef {
    LanguageDef {
        name: "Perl",
        keywords: [
            "and", "cmp", "continue", "do", "else", "elsif", "eq", "for",
            "foreach", "ge", "goto", "gt", "if", "last", "le", "lt", "my",
            "ne", "next", "no", "or", "our", "package", "redo", "require",
            "return", "sub", "unless", "until", "use", "while", "xor",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: Some("=pod"),
        block_comment_end: Some("=cut"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "->", "++", "--", "**", "=~", "!~", "<=", ">=", "==", "!=",
            "<=>", "&&", "||", "..", "...",
            "+", "-", "*", "/", "%", ".", "<", ">", "&", "|", "^", "~", "!",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', ':', '$', '@', '%'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn r_def() -> LanguageDef {
    LanguageDef {
        name: "R",
        keywords: [
            "break", "else", "for", "function", "if", "in", "next", "repeat",
            "return", "while", "TRUE", "FALSE", "NULL", "NA", "NA_integer_",
            "NA_real_", "NA_complex_", "NA_character_", "Inf", "NaN",
        ].into_iter().collect(),
        types: [
            "character", "complex", "double", "expression", "integer", "list",
            "logical", "numeric", "raw", "vector", "matrix", "array",
            "data.frame", "factor",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "<-", "->", "<<-", "->>", "%%", "%/%", "%*%", "%in%", "%o%", "%x%",
            "<=", ">=", "==", "!=", "&&", "||", "!", "&", "|",
            "+", "-", "*", "/", "^", ":", "$", "@", "~",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ','],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn julia_def() -> LanguageDef {
    LanguageDef {
        name: "Julia",
        keywords: [
            "abstract", "baremodule", "begin", "break", "catch", "const",
            "continue", "do", "else", "elseif", "end", "export", "false",
            "finally", "for", "function", "global", "if", "import", "in",
            "let", "local", "macro", "module", "mutable", "primitive", "quote",
            "return", "struct", "true", "try", "type", "using", "where", "while",
        ].into_iter().collect(),
        types: [
            "Any", "Bool", "Char", "Complex", "Float16", "Float32", "Float64",
            "Int", "Int8", "Int16", "Int32", "Int64", "Int128", "Integer",
            "Nothing", "Number", "Rational", "Real", "String", "Symbol",
            "UInt", "UInt8", "UInt16", "UInt32", "UInt64", "UInt128",
            "Array", "Dict", "Set", "Tuple", "Vector", "Matrix",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: Some("#="),
        block_comment_end: Some("=#"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "->", "=>", "::", "...", "..", "<=", ">=", "==", "!=", "===", "!==",
            "&&", "||", "⊻", "<<", ">>", ">>>", "+=", "-=", "*=", "/=",
            "+", "-", "*", "/", "\\", "^", "%", "&", "|", "~", "<", ">", "!",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn elixir_def() -> LanguageDef {
    LanguageDef {
        name: "Elixir",
        keywords: [
            "after", "alias", "and", "case", "catch", "cond", "def", "defp",
            "defcallback", "defdelegate", "defexception", "defguard",
            "defguardp", "defimpl", "defmacro", "defmacrop", "defmodule",
            "defoverridable", "defprotocol", "defstruct", "do", "else", "end",
            "false", "fn", "for", "if", "import", "in", "nil", "not", "or",
            "quote", "raise", "receive", "require", "rescue", "true", "try",
            "unless", "unquote", "unquote_splicing", "use", "when", "with",
        ].into_iter().collect(),
        types: [
            "Atom", "BitString", "Float", "Function", "Integer", "List", "Map",
            "PID", "Port", "Reference", "Tuple", "String", "Keyword",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "->", "<-", "|>", "++", "--", "<>", "..", "<=", ">=", "==", "!=",
            "===", "!==", "&&", "||", "and", "or", "not", "in",
            "+", "-", "*", "/", "^", "&", "|", "~", "<", ">", "=", "!",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':', '@', '%'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn erlang_def() -> LanguageDef {
    LanguageDef {
        name: "Erlang",
        keywords: [
            "after", "and", "andalso", "band", "begin", "bnot", "bor", "bsl",
            "bsr", "bxor", "case", "catch", "cond", "div", "end", "fun", "if",
            "let", "not", "of", "or", "orelse", "receive", "rem", "try", "when",
            "xor",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("%"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "->", "<-", "++", "--", "==", "/=", "=<", ">=", "=:=", "=/=",
            "||", "!", "+", "-", "*", "/", "<", ">", "=",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':', '|'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn clojure_def() -> LanguageDef {
    LanguageDef {
        name: "Clojure",
        keywords: [
            "def", "defn", "defn-", "defmacro", "defmulti", "defmethod",
            "defprotocol", "defrecord", "defstruct", "deftype", "fn", "if",
            "do", "let", "loop", "recur", "throw", "try", "catch", "finally",
            "quote", "var", "import", "use", "require", "ns", "in-ns", "new",
            "set!", "nil", "true", "false",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some(";"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"'],
        multiline_strings: true,
        operators: vec![
            "->", "->>", "=>", "@", "^", "`", "~", "~@", "#",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ':', '\''],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn fortran_def() -> LanguageDef {
    LanguageDef {
        name: "Fortran",
        keywords: [
            "allocatable", "allocate", "assign", "associate", "asynchronous",
            "backspace", "block", "call", "case", "character", "class", "close",
            "codimension", "common", "complex", "concurrent", "contains",
            "contiguous", "continue", "critical", "cycle", "data", "deallocate",
            "default", "deferred", "dimension", "do", "double", "elemental",
            "else", "elseif", "elsewhere", "end", "endfile", "endif", "entry",
            "enum", "enumerator", "equivalence", "error", "exit", "extends",
            "external", "final", "flush", "forall", "format", "function",
            "generic", "go", "goto", "if", "images", "implicit", "import",
            "include", "inquire", "intent", "interface", "intrinsic", "kind",
            "len", "lock", "logical", "module", "namelist", "none", "nopass",
            "nullify", "only", "open", "operator", "optional", "out", "parameter",
            "pass", "pause", "pointer", "precision", "print", "private",
            "procedure", "program", "protected", "public", "pure", "read",
            "real", "recursive", "result", "return", "rewind", "save", "select",
            "sequence", "stop", "submodule", "subroutine", "sync", "target",
            "then", "to", "type", "unlock", "use", "value", "volatile", "wait",
            "where", "while", "write",
        ].into_iter().collect(),
        types: [
            "integer", "real", "complex", "character", "logical", "double",
            "precision", "type", "class",
        ].into_iter().collect(),
        line_comment: Some("!"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "==", "/=", "<=", ">=", "**", "//", ".eq.", ".ne.", ".lt.", ".le.",
            ".gt.", ".ge.", ".and.", ".or.", ".not.", ".eqv.", ".neqv.",
            "+", "-", "*", "/", "<", ">", "=",
        ],
        punctuation: vec!['(', ')', '[', ']', ',', ':', '%'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn zig_def() -> LanguageDef {
    LanguageDef {
        name: "Zig",
        keywords: [
            "addrspace", "align", "allowzero", "and", "anyframe", "anytype",
            "asm", "async", "await", "break", "callconv", "catch", "comptime",
            "const", "continue", "defer", "else", "enum", "errdefer", "error",
            "export", "extern", "fn", "for", "if", "inline", "linksection",
            "noalias", "noinline", "nosuspend", "opaque", "or", "orelse",
            "packed", "pub", "resume", "return", "struct", "suspend", "switch",
            "test", "threadlocal", "try", "union", "unreachable", "usingnamespace",
            "var", "volatile", "while", "null", "undefined", "true", "false",
        ].into_iter().collect(),
        types: [
            "i8", "u8", "i16", "u16", "i32", "u32", "i64", "u64", "i128", "u128",
            "isize", "usize", "f16", "f32", "f64", "f80", "f128", "bool", "void",
            "anyerror", "anyopaque", "comptime_int", "comptime_float", "type",
            "noreturn",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"'],
        multiline_strings: false,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn nim_def() -> LanguageDef {
    LanguageDef {
        name: "Nim",
        keywords: [
            "addr", "and", "as", "asm", "bind", "block", "break", "case", "cast",
            "concept", "const", "continue", "converter", "defer", "discard",
            "distinct", "div", "do", "elif", "else", "end", "enum", "except",
            "export", "finally", "for", "from", "func", "if", "import", "in",
            "include", "interface", "is", "isnot", "iterator", "let", "macro",
            "method", "mixin", "mod", "nil", "not", "notin", "object", "of",
            "or", "out", "proc", "ptr", "raise", "ref", "return", "shl", "shr",
            "static", "template", "try", "tuple", "type", "using", "var", "when",
            "while", "xor", "yield", "true", "false",
        ].into_iter().collect(),
        types: [
            "int", "int8", "int16", "int32", "int64", "uint", "uint8", "uint16",
            "uint32", "uint64", "float", "float32", "float64", "bool", "char",
            "string", "cstring", "pointer", "seq", "array", "set", "tuple",
            "object", "ref", "ptr", "proc", "iterator", "void",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: Some("#["),
        block_comment_end: Some("]#"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "==", "!=", "<=", ">=", "<", ">", "+=", "-=", "*=", "/=", "&=",
            "and", "or", "not", "xor", "shl", "shr", "div", "mod",
            "+", "-", "*", "/", "&", "|", "^", "~", "@", "$",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn odin_def() -> LanguageDef {
    LanguageDef {
        name: "Odin",
        keywords: [
            "align_of", "auto_cast", "bit_set", "break", "case", "cast",
            "context", "continue", "defer", "distinct", "do", "dynamic",
            "else", "enum", "fallthrough", "false", "for", "foreign", "if",
            "import", "in", "map", "matrix", "nil", "not_in", "offset_of",
            "or_else", "or_return", "package", "proc", "return", "size_of",
            "struct", "switch", "transmute", "true", "type_of", "typeid",
            "union", "using", "when", "where",
        ].into_iter().collect(),
        types: [
            "bool", "b8", "b16", "b32", "b64", "int", "i8", "i16", "i32", "i64",
            "i128", "uint", "u8", "u16", "u32", "u64", "u128", "uintptr",
            "f16", "f32", "f64", "complex32", "complex64", "complex128",
            "quaternion64", "quaternion128", "quaternion256", "string", "cstring",
            "rune", "rawptr", "typeid", "any",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn v_def() -> LanguageDef {
    LanguageDef {
        name: "V",
        keywords: [
            "as", "asm", "assert", "atomic", "break", "const", "continue",
            "defer", "else", "enum", "false", "fn", "for", "go", "goto", "if",
            "import", "in", "interface", "is", "isreftype", "lock", "match",
            "module", "mut", "none", "or", "pub", "return", "rlock", "select",
            "shared", "sizeof", "spawn", "static", "struct", "true", "type",
            "typeof", "union", "unsafe", "volatile", "__offsetof",
        ].into_iter().collect(),
        types: [
            "bool", "string", "i8", "i16", "int", "i64", "i128", "u8", "u16",
            "u32", "u64", "u128", "rune", "f32", "f64", "isize", "usize",
            "voidptr", "any", "thread",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn d_def() -> LanguageDef {
    LanguageDef {
        name: "D",
        keywords: [
            "abstract", "alias", "align", "asm", "assert", "auto", "body",
            "bool", "break", "case", "cast", "catch", "class", "const",
            "continue", "debug", "default", "delegate", "delete", "deprecated",
            "do", "else", "enum", "export", "extern", "false", "final",
            "finally", "for", "foreach", "foreach_reverse", "function", "goto",
            "if", "immutable", "import", "in", "inout", "interface", "invariant",
            "is", "lazy", "mixin", "module", "new", "nothrow", "null", "out",
            "override", "package", "pragma", "private", "protected", "public",
            "pure", "ref", "return", "scope", "shared", "static", "struct",
            "super", "switch", "synchronized", "template", "this", "throw",
            "true", "try", "typeid", "typeof", "union", "unittest", "version",
            "while", "with", "__FILE__", "__LINE__", "__gshared", "__traits",
        ].into_iter().collect(),
        types: [
            "void", "bool", "byte", "ubyte", "short", "ushort", "int", "uint",
            "long", "ulong", "cent", "ucent", "float", "double", "real",
            "ifloat", "idouble", "ireal", "cfloat", "cdouble", "creal", "char",
            "wchar", "dchar", "string", "wstring", "dstring", "size_t", "ptrdiff_t",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\'', '`'],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn bash_def() -> LanguageDef {
    LanguageDef {
        name: "Bash",
        keywords: [
            "if", "then", "else", "elif", "fi", "case", "esac", "for", "while",
            "until", "do", "done", "in", "function", "select", "time", "coproc",
            "break", "continue", "return", "exit", "export", "readonly", "local",
            "declare", "typeset", "unset", "shift", "source", "alias", "unalias",
            "set", "shopt", "trap", "eval", "exec", "true", "false",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "||", "&&", ";;", ";&", ";;&", "|&", "<<", ">>", "<&", ">&", "<>",
            "==", "!=", "<=", ">=", "-eq", "-ne", "-lt", "-le", "-gt", "-ge",
            "-z", "-n", "-e", "-f", "-d", "-r", "-w", "-x",
            "|", "&", ";", "<", ">", "=",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', '$', '`'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn fish_def() -> LanguageDef {
    LanguageDef {
        name: "Fish",
        keywords: [
            "and", "begin", "break", "builtin", "case", "command", "continue",
            "else", "end", "exec", "for", "function", "if", "in", "not", "or",
            "return", "set", "status", "switch", "test", "while",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![
            "||", "&&", ";", "|", "&", "<", ">", "=",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', '$'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn zsh_def() -> LanguageDef {
    let mut def = bash_def();
    def.name = "Zsh";
    def.keywords.extend([
        "autoload", "bindkey", "compdef", "compadd", "compinit", "emulate",
        "noglob", "zle", "zmodload", "zstyle",
    ]);
    def
}

fn powershell_def() -> LanguageDef {
    LanguageDef {
        name: "PowerShell",
        keywords: [
            "begin", "break", "catch", "class", "continue", "data", "define",
            "do", "dynamicparam", "else", "elseif", "end", "enum", "exit",
            "filter", "finally", "for", "foreach", "from", "function", "hidden",
            "if", "in", "inlinescript", "param", "process", "return", "static",
            "switch", "throw", "trap", "try", "until", "using", "var", "while",
            "workflow", "parallel", "sequence",
        ].into_iter().collect(),
        types: [
            "bool", "byte", "char", "datetime", "decimal", "double", "float",
            "int", "long", "object", "sbyte", "short", "single", "string",
            "uint", "ulong", "ushort", "void", "array", "hashtable", "xml",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: Some("<#"),
        block_comment_end: Some("#>"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![
            "-eq", "-ne", "-gt", "-ge", "-lt", "-le", "-like", "-notlike",
            "-match", "-notmatch", "-contains", "-notcontains", "-in", "-notin",
            "-replace", "-split", "-join", "-and", "-or", "-xor", "-not", "-band",
            "-bor", "-bxor", "-bnot", "-shl", "-shr",
            "=", "+=", "-=", "*=", "/=", "%=", "++", "--",
            "|", "&", ";", "<", ">",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', '$', '@', ',', '.'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn sql_def() -> LanguageDef {
    LanguageDef {
        name: "SQL",
        keywords: [
            "add", "all", "alter", "and", "any", "as", "asc", "backup", "between",
            "by", "case", "check", "column", "constraint", "create", "database",
            "default", "delete", "desc", "distinct", "drop", "exec", "exists",
            "foreign", "from", "full", "group", "having", "in", "index", "inner",
            "insert", "into", "is", "join", "key", "left", "like", "limit",
            "not", "null", "on", "or", "order", "outer", "primary", "procedure",
            "right", "rownum", "select", "set", "table", "top", "truncate",
            "union", "unique", "update", "values", "view", "where", "with",
            "true", "false", "begin", "end", "declare", "if", "else", "while",
            "return", "commit", "rollback", "transaction",
        ].into_iter().collect(),
        types: [
            "int", "integer", "smallint", "bigint", "decimal", "numeric", "float",
            "real", "double", "precision", "char", "varchar", "text", "nchar",
            "nvarchar", "ntext", "binary", "varbinary", "image", "date", "time",
            "datetime", "timestamp", "boolean", "bool", "bit", "money", "xml",
            "json", "uuid", "serial", "bytea", "array",
        ].into_iter().collect(),
        line_comment: Some("--"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['\''],
        multiline_strings: false,
        operators: vec![
            "<=", ">=", "<>", "!=", "||", "::", "->", "->>",
            "+", "-", "*", "/", "%", "&", "|", "^", "~", "<", ">", "=",
        ],
        punctuation: vec!['(', ')', ',', '.', ';', ':'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn html_def() -> LanguageDef {
    LanguageDef {
        name: "HTML",
        keywords: HashSet::new(),
        types: HashSet::new(),
        line_comment: None,
        block_comment_start: Some("<!--"),
        block_comment_end: Some("-->"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec!["="],
        punctuation: vec!['<', '>', '/', '!'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn css_def() -> LanguageDef {
    LanguageDef {
        name: "CSS",
        keywords: [
            "!important", "@charset", "@font-face", "@import", "@keyframes",
            "@media", "@namespace", "@page", "@supports", "@viewport",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: None,
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec![":", ";", ",", "+", ">", "~", "*"],
        punctuation: vec!['{', '}', '(', ')', '[', ']', '.', '#'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn json_def() -> LanguageDef {
    LanguageDef {
        name: "JSON",
        keywords: ["true", "false", "null"].into_iter().collect(),
        types: HashSet::new(),
        line_comment: None,
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"'],
        multiline_strings: false,
        operators: vec![":"],
        punctuation: vec!['{', '}', '[', ']', ','],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn yaml_def() -> LanguageDef {
    LanguageDef {
        name: "YAML",
        keywords: [
            "true", "false", "null", "yes", "no", "on", "off", "~",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec![":", "-", ">", "|", "&", "*", "!"],
        punctuation: vec!['{', '}', '[', ']', ','],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn toml_def() -> LanguageDef {
    LanguageDef {
        name: "TOML",
        keywords: ["true", "false"].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: vec!["="],
        punctuation: vec!['{', '}', '[', ']', ',', '.'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn xml_def() -> LanguageDef {
    LanguageDef {
        name: "XML",
        keywords: HashSet::new(),
        types: HashSet::new(),
        line_comment: None,
        block_comment_start: Some("<!--"),
        block_comment_end: Some("-->"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec!["="],
        punctuation: vec!['<', '>', '/', '?', '!'],
        has_preprocessor: true, // <?xml ... ?>
        case_sensitive: true,
    }
}

fn markdown_def() -> LanguageDef {
    LanguageDef {
        name: "Markdown",
        keywords: HashSet::new(),
        types: HashSet::new(),
        line_comment: None,
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec![],
        multiline_strings: false,
        operators: vec!["#", "*", "-", "+", ">", "`", "~", "_", "[", "]", "(", ")"],
        punctuation: vec![],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn makefile_def() -> LanguageDef {
    LanguageDef {
        name: "Makefile",
        keywords: [
            "ifeq", "ifneq", "ifdef", "ifndef", "else", "endif", "define",
            "endef", "export", "unexport", "override", "include", "-include",
            "sinclude", "vpath", ".PHONY", ".SUFFIXES", ".DEFAULT", ".PRECIOUS",
            ".INTERMEDIATE", ".SECONDARY", ".SECONDEXPANSION", ".DELETE_ON_ERROR",
            ".IGNORE", ".LOW_RESOLUTION_TIME", ".SILENT", ".EXPORT_ALL_VARIABLES",
            ".NOTPARALLEL", ".ONESHELL", ".POSIX",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec!["=", ":=", "?=", "+=", "::=", "!=", ":", "|", ";", "@", "-"],
        punctuation: vec!['$', '(', ')', '{', '}', '%', '*', '?', '<', '>'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn dockerfile_def() -> LanguageDef {
    LanguageDef {
        name: "Dockerfile",
        keywords: [
            "ADD", "ARG", "CMD", "COPY", "ENTRYPOINT", "ENV", "EXPOSE", "FROM",
            "HEALTHCHECK", "LABEL", "MAINTAINER", "ONBUILD", "RUN", "SHELL",
            "STOPSIGNAL", "USER", "VOLUME", "WORKDIR", "AS",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: None,
        block_comment_end: None,
        string_delimiters: vec!['"', '\''],
        multiline_strings: false,
        operators: vec!["=", "\\"],
        punctuation: vec!['[', ']', '{', '}', '$'],
        has_preprocessor: false,
        case_sensitive: false,
    }
}

fn terraform_def() -> LanguageDef {
    LanguageDef {
        name: "Terraform",
        keywords: [
            "data", "locals", "module", "output", "provider", "resource",
            "terraform", "variable", "for", "for_each", "if", "in", "dynamic",
            "content", "count", "depends_on", "lifecycle", "provisioner",
            "connection", "null_resource", "true", "false", "null",
        ].into_iter().collect(),
        types: [
            "string", "number", "bool", "list", "map", "set", "object", "tuple",
            "any",
        ].into_iter().collect(),
        line_comment: Some("#"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"'],
        multiline_strings: true,
        operators: vec!["=", "=>", "==", "!=", "<=", ">=", "&&", "||", "!", "?", ":"],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ',', '.'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn nix_def() -> LanguageDef {
    LanguageDef {
        name: "Nix",
        keywords: [
            "assert", "else", "if", "import", "in", "inherit", "let", "or",
            "rec", "then", "with", "true", "false", "null",
        ].into_iter().collect(),
        types: HashSet::new(),
        line_comment: Some("#"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"'],
        multiline_strings: true,
        operators: vec!["=", "++", "//", "->", ":", "?", "@", "==", "!=", "<=", ">=", "&&", "||", "!"],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ',', '.', ';'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn ocaml_def() -> LanguageDef {
    LanguageDef {
        name: "OCaml",
        keywords: [
            "and", "as", "assert", "asr", "begin", "class", "constraint", "do",
            "done", "downto", "else", "end", "exception", "external", "false",
            "for", "fun", "function", "functor", "if", "in", "include",
            "inherit", "initializer", "land", "lazy", "let", "lor", "lsl",
            "lsr", "lxor", "match", "method", "mod", "module", "mutable", "new",
            "nonrec", "object", "of", "open", "or", "private", "rec", "sig",
            "struct", "then", "to", "true", "try", "type", "val", "virtual",
            "when", "while", "with",
        ].into_iter().collect(),
        types: [
            "int", "float", "bool", "char", "string", "bytes", "unit", "exn",
            "array", "list", "option", "ref", "lazy_t",
        ].into_iter().collect(),
        line_comment: None,
        block_comment_start: Some("(*"),
        block_comment_end: Some("*)"),
        string_delimiters: vec!['"'],
        multiline_strings: false,
        operators: vec![
            "->", "<-", "|>", "@@", "::", "@", "^", "||", "&&", "==", "!=",
            "<=", ">=", "<>", ":=", "++", "--",
            "+", "-", "*", "/", "~", "!", "<", ">", "|", "&", "=",
        ],
        punctuation: vec!['{', '}', '(', ')', '[', ']', ';', ',', '.', ':'],
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn fsharp_def() -> LanguageDef {
    let mut def = ocaml_def();
    def.name = "F#";
    def.keywords.extend([
        "abstract", "base", "default", "delegate", "elif", "elif", "fixed",
        "global", "inline", "interface", "internal", "member", "namespace",
        "null", "override", "public", "return", "static", "upcast", "use",
        "void", "yield",
    ]);
    def.line_comment = Some("//");
    def
}

fn dart_def() -> LanguageDef {
    LanguageDef {
        name: "Dart",
        keywords: [
            "abstract", "as", "assert", "async", "await", "break", "case",
            "catch", "class", "const", "continue", "covariant", "default",
            "deferred", "do", "dynamic", "else", "enum", "export", "extends",
            "extension", "external", "factory", "false", "final", "finally",
            "for", "Function", "get", "hide", "if", "implements", "import",
            "in", "interface", "is", "late", "library", "mixin", "new", "null",
            "on", "operator", "part", "required", "rethrow", "return", "set",
            "show", "static", "super", "switch", "sync", "this", "throw",
            "true", "try", "typedef", "var", "void", "while", "with", "yield",
        ].into_iter().collect(),
        types: [
            "bool", "double", "dynamic", "int", "List", "Map", "Never", "Null",
            "num", "Object", "Set", "String", "Symbol", "Type", "void",
            "Future", "Stream", "Iterable", "Iterator",
        ].into_iter().collect(),
        line_comment: Some("//"),
        block_comment_start: Some("/*"),
        block_comment_end: Some("*/"),
        string_delimiters: vec!['"', '\''],
        multiline_strings: true,
        operators: C_OPERATORS.to_vec(),
        punctuation: C_PUNCTUATION.to_vec(),
        has_preprocessor: false,
        case_sensitive: true,
    }
}

fn groovy_def() -> LanguageDef {
    let mut def = java_def();
    def.name = "Groovy";
    def.keywords.extend([
        "as", "def", "in", "trait",
    ]);
    def.multiline_strings = true;
    def
}
