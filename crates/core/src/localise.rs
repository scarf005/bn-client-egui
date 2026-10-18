//! Renders the engine's LocalisedStrings: the message is looked up in the engine's own gettext catalogue
//! (`lang/mo/<locale>/LC_MESSAGES/cataclysm-bn.mo` of the game root), its plural form comes from the
//! catalogue header, and its placeholders are filled the way the engine's `string_format` fills them.
//!
//! The client words its own chrome with `tr` and its kin from its built-in catalogues (`crates/core/po`), in
//! the same language and with the same plural and placeholder rules.
//!
//! The catalogues of the connection are installed per thread, by the thread that polls the model, which is
//! also the thread that draws: `Text::shown` and `tr` read them without a handle.

use crate::protocol::{LocalisedString, TextArg};
use serde_json::Number;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The engine's catalogue for one language.
///
/// Hand-written: `gettext` 0.4 resolves the plural form of a rule such as Russian's wrongly (its parser splits
/// `&&` out before `?:`), and `polib` only writes `.mo` files.
pub struct Catalogue {
    /// Every entry by its msgid, `context\u{4}msgid` for one with a context: its forms, singular first.
    entries: HashMap<String, Vec<String>>,
    plural: Plural,
}

impl Catalogue {
    /// Parses the bytes of a `.mo` file.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let word = |at: usize, big: bool| -> Result<usize, String> {
            let raw = bytes.get(at..at + 4).ok_or("truncated catalogue")?;
            let raw = [raw[0], raw[1], raw[2], raw[3]];
            Ok(if big {
                u32::from_be_bytes(raw)
            } else {
                u32::from_le_bytes(raw)
            } as usize)
        };
        let big = match word(0, false)? {
            0x950412de => false,
            0xde120495 => true,
            _ => return Err("not a catalogue".into()),
        };
        let (count, originals, translations) = (word(8, big)?, word(12, big)?, word(16, big)?);
        let string = |table: usize, index: usize| -> Result<&str, String> {
            let (length, offset) = (
                word(table + 8 * index, big)?,
                word(table + 8 * index + 4, big)?,
            );
            let raw = bytes
                .get(offset..offset + length)
                .ok_or("truncated catalogue")?;
            std::str::from_utf8(raw).map_err(|e| e.to_string())
        };
        let mut entries = HashMap::new();
        let mut plural = Plural::default();
        for index in 0..count {
            let (id, forms) = (string(originals, index)?, string(translations, index)?);
            if id.is_empty() {
                plural = forms
                    .lines()
                    .find_map(|line| line.strip_prefix("Plural-Forms:"))
                    .and_then(|rules| {
                        rules
                            .split(';')
                            .find_map(|rule| rule.trim().strip_prefix("plural="))
                    })
                    .map_or(Ok(Plural::default()), Plural::parse)?;
            } else {
                let singular = id.split('\0').next().unwrap_or(id);
                entries.insert(
                    singular.to_string(),
                    forms.split('\0').map(String::from).collect(),
                );
            }
        }
        Ok(Self { entries, plural })
    }

    /// The catalogue of `locale` under the game root; none for the source language, which has no file, and
    /// for a language that is not compiled.
    pub fn load(root: &Path, locale: &str) -> Option<Self> {
        let path = names(locale)
            .map(|name| {
                root.join("lang/mo")
                    .join(name)
                    .join("LC_MESSAGES/cataclysm-bn.mo")
            })
            .find(|path| path.is_file())?;
        Self::parse(&std::fs::read(path).ok()?).ok()
    }

    /// The client's own catalogue of `locale`, built in; none for a language it has no words for.
    pub fn client(locale: &str) -> Option<Arc<Self>> {
        names(locale)
            .find_map(|name| CLIENT.iter().find(|(language, _)| *language == name))
            .and_then(|(_, bytes)| Self::parse(bytes).ok())
            .map(Arc::new)
    }

    /// The format of `message` in this language: the translation, or its key when the catalogue has none.
    fn format<'a>(&'a self, message: &'a LocalisedString) -> &'a str {
        let key = message.key.as_str();
        let id = message
            .ctx
            .as_ref()
            .map_or_else(|| key.to_string(), |ctx| format!("{ctx}\u{4}{key}"));
        self.translation(&id, message.plural.as_ref().map(count))
            .unwrap_or(key)
    }

    /// The translation of the entry `id`, in the form for the count `n`.
    fn translation(&self, id: &str, n: Option<u64>) -> Option<&str> {
        let form = n.map_or(0, |n| self.plural.form(n));
        self.entries
            .get(id)
            .and_then(|forms| forms.get(form).or(forms.first()))
            .map(String::as_str)
    }
}

/// The catalogue names a BN language id is looked up by, as the engine looks up its own: the id itself, then
/// the language without its dialect (`ko_KR`, then `ko`).
fn names(locale: &str) -> impl Iterator<Item = &str> {
    let language = locale.split_once('_').map(|(language, _)| language);
    std::iter::once(locale).chain(language)
}

/// The client's catalogues by locale, compiled from `crates/core/po` by the build script.
const CLIENT: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/catalogues.rs"));

/// The plural rule of a catalogue: the C expression of its `Plural-Forms` header over `n`.
#[derive(Default)]
struct Plural(Option<Expr>);

#[derive(Debug)]
enum Expr {
    N,
    Number(u64),
    Not(Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Plural {
    fn parse(source: &str) -> Result<Self, String> {
        let mut parser = Parser {
            tokens: tokens(source)?,
            at: 0,
        };
        let expr = parser.ternary()?;
        if parser.at < parser.tokens.len() {
            return Err(format!(
                "unexpected \"{}\" in plural rule",
                parser.tokens[parser.at]
            ));
        }
        Ok(Self(Some(expr)))
    }

    /// The index of the form for the count `n`; the first when the catalogue has no rule.
    fn form(&self, n: u64) -> usize {
        self.0.as_ref().map_or(0, |expr| expr.eval(n) as usize)
    }
}

impl Expr {
    fn eval(&self, n: u64) -> u64 {
        match self {
            Expr::N => n,
            Expr::Number(value) => *value,
            Expr::Not(operand) => u64::from(operand.eval(n) == 0),
            Expr::Ternary(condition, yes, no) => {
                if condition.eval(n) != 0 { yes } else { no }.eval(n)
            }
            Expr::Binary(op, left, right) => {
                let (l, r) = (left.eval(n), right.eval(n));
                match *op {
                    "||" => u64::from(l != 0 || r != 0),
                    "&&" => u64::from(l != 0 && r != 0),
                    "==" => u64::from(l == r),
                    "!=" => u64::from(l != r),
                    "<" => u64::from(l < r),
                    ">" => u64::from(l > r),
                    "<=" => u64::from(l <= r),
                    ">=" => u64::from(l >= r),
                    "+" => l.wrapping_add(r),
                    "-" => l.wrapping_sub(r),
                    "*" => l.wrapping_mul(r),
                    "/" => l.checked_div(r).unwrap_or(0),
                    _ => l.checked_rem(r).unwrap_or(0),
                }
            }
        }
    }
}

fn tokens(source: &str) -> Result<Vec<String>, String> {
    let (mut out, mut chars) = (vec![], source.chars().peekable());
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
        } else if c.is_ascii_digit() {
            let mut digits = c.to_string();
            while let Some(d) = chars.next_if(char::is_ascii_digit) {
                digits.push(d);
            }
            out.push(digits);
        } else if "n()?:+-*/%".contains(c) {
            out.push(c.to_string());
        } else if "&|=".contains(c) && chars.next_if_eq(&c).is_some() {
            out.push(format!("{c}{c}"));
        } else if "!<>".contains(c) {
            out.push(if chars.next_if_eq(&'=').is_some() {
                format!("{c}=")
            } else {
                c.to_string()
            });
        } else {
            return Err(format!("unexpected \"{c}\" in plural rule"));
        }
    }
    Ok(out)
}

/// Binary operators from the loosest to the tightest, as in C.
const LEVELS: [&[&str]; 6] = [
    &["||"],
    &["&&"],
    &["==", "!="],
    &["<", ">", "<=", ">="],
    &["+", "-"],
    &["*", "/", "%"],
];

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn take(&mut self, token: &str) -> bool {
        let found = self.peek() == Some(token);
        self.at += usize::from(found);
        found
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        let condition = self.binary(0)?;
        if !self.take("?") {
            return Ok(condition);
        }
        let yes = self.ternary()?;
        if !self.take(":") {
            return Err("missing \":\" in plural rule".into());
        }
        Ok(Expr::Ternary(
            Box::new(condition),
            Box::new(yes),
            Box::new(self.ternary()?),
        ))
    }

    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        let Some(operators) = LEVELS.get(level) else {
            return self.unary();
        };
        let mut left = self.binary(level + 1)?;
        while let Some(op) = operators.iter().find(|op| self.peek() == Some(**op)) {
            self.at += 1;
            left = Expr::Binary(op, Box::new(left), Box::new(self.binary(level + 1)?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.take("!") {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        if self.take("(") {
            let inner = self.ternary()?;
            return if self.take(")") {
                Ok(inner)
            } else {
                Err("missing \")\" in plural rule".into())
            };
        }
        let token = self.peek().ok_or("plural rule ends early")?;
        let expr = match token {
            "n" => Expr::N,
            digits => Expr::Number(
                digits
                    .parse()
                    .map_err(|_| format!("unexpected \"{digits}\" in plural rule"))?,
            ),
        };
        self.at += 1;
        Ok(expr)
    }
}

/// The count that selects a plural form: negative counts and fractions select as their magnitude's integer part.
fn count(n: &Number) -> u64 {
    n.as_u64()
        .or_else(|| n.as_i64().map(i64::unsigned_abs))
        .unwrap_or_else(|| n.as_f64().map_or(0, |f| f.abs() as u64))
}

thread_local! {
    static ACTIVE: RefCell<Option<Arc<Catalogue>>> = const { RefCell::new(None) };
    static CHROME: RefCell<Option<Arc<Catalogue>>> = const { RefCell::new(None) };
}

/// Makes `catalogue` the one this thread renders with; none renders every message from its key.
pub fn install(catalogue: Option<Arc<Catalogue>>) {
    ACTIVE.with(|active| *active.borrow_mut() = catalogue);
}

/// Makes `catalogue` the one this thread words the client's chrome with; none words it in English.
pub fn install_client(catalogue: Option<Arc<Catalogue>>) {
    CHROME.with(|chrome| *chrome.borrow_mut() = catalogue);
}

/// A value of a placeholder.
pub enum Value {
    Int(i64),
    Float(f64),
    Text(String),
}

/// The client's words `msgid` in this thread's language.
pub fn tr(msgid: &str) -> String {
    tr_format(msgid, &[])
}

/// The client's words `msgid` in this thread's language, placeholders filled.
pub fn tr_format(msgid: &str, args: &[Value]) -> String {
    let format = CHROME.with(|chrome| {
        let chrome = chrome.borrow();
        chrome
            .as_deref()
            .and_then(|c| c.translation(msgid, None))
            .map(String::from)
    });
    printf(format.as_deref().unwrap_or(msgid), args)
}

/// The client's words for the count `n` in this thread's language, placeholders filled: `one` or `other` in
/// English.
pub fn tr_plural(one: &str, other: &str, n: u64, args: &[Value]) -> String {
    let format = CHROME.with(|chrome| {
        let chrome = chrome.borrow();
        chrome
            .as_deref()
            .and_then(|c| c.translation(one, Some(n)))
            .map(String::from)
    });
    let english = if n == 1 { one } else { other };
    printf(format.as_deref().unwrap_or(english), args)
}

fn value(arg: &TextArg) -> Value {
    match arg {
        TextArg::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or(Value::Int(0)),
        TextArg::Text(text) => Value::Text(text.shown()),
    }
}

/// The message in this thread's language, placeholders filled. The empty key joins its arguments.
pub fn render(message: &LocalisedString) -> String {
    let args = message
        .args
        .iter()
        .flatten()
        .map(value)
        .collect::<Vec<Value>>();
    if message.key.is_empty() {
        return args.iter().map(|arg| display(arg, None)).collect();
    }
    ACTIVE.with(|active| match active.borrow().as_deref() {
        Some(catalogue) => printf(catalogue.format(message), &args),
        None => printf(&message.key, &args),
    })
}

fn display(value: &Value, precision: Option<usize>) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::Float(f) => precision.map_or_else(|| f.to_string(), |p| format!("{f:.p$}")),
        Value::Text(text) => text.clone(),
    }
}

/// One `%[n$][flags][width][.precision][length]conversion`.
#[derive(Default)]
struct Spec {
    position: Option<usize>,
    left: bool,
    zero: bool,
    plus: bool,
    space: bool,
    width: usize,
    precision: Option<usize>,
}

/// Reads the digits at the front of `chars`.
fn number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<usize> {
    let mut digits = String::new();
    while let Some(digit) = chars.next_if(char::is_ascii_digit) {
        digits.push(digit);
    }
    digits.parse().ok()
}

/// The text of the format with its placeholders replaced: `%s`, `%d`, `%i`, `%u`, `%x`, `%X`, `%o`, `%f`,
/// `%c`, `%%` and the positional `%1$s`, with flags, width and precision. A placeholder without a value
/// shows nothing.
fn printf(format: &str, args: &[Value]) -> String {
    let (mut out, mut chars, mut next) = (String::new(), format.chars().peekable(), 0);
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.next_if_eq(&'%').is_some() {
            out.push('%');
            continue;
        }
        let mut spec = Spec::default();
        // A position is digits and a dollar sign; a leading zero is a flag.
        let mut ahead = chars.clone();
        if ahead.peek().is_some_and(|c| ('1'..='9').contains(c))
            && let Some(position) = number(&mut ahead)
            && ahead.next_if_eq(&'$').is_some()
        {
            spec.position = Some(position);
            chars = ahead;
        }
        while let Some(flag) = chars.next_if(|c| "-0+ #".contains(*c)) {
            match flag {
                '-' => spec.left = true,
                '0' => spec.zero = true,
                '+' => spec.plus = true,
                ' ' => spec.space = true,
                _ => {}
            }
        }
        let lead = number(&mut chars);
        spec.width = lead.unwrap_or(0);
        if chars.next_if_eq(&'.').is_some() {
            spec.precision = Some(number(&mut chars).unwrap_or(0));
        }
        while chars.next_if(|c| "hlLzjtq".contains(*c)).is_some() {}
        let Some(conversion) = chars.next() else {
            break;
        };
        let index = spec.position.map_or_else(
            || {
                next += 1;
                next - 1
            },
            |position| position.saturating_sub(1),
        );
        if let Some(arg) = args.get(index) {
            out.push_str(&pad(&convert(conversion, arg, &spec), &spec, conversion));
        }
    }
    out
}

fn convert(conversion: char, arg: &Value, spec: &Spec) -> String {
    let integer = || match arg {
        Value::Int(n) => *n,
        Value::Float(f) => *f as i64,
        Value::Text(text) => text.parse().unwrap_or(0),
    };
    let sign = |negative: bool| match (negative, spec.plus, spec.space) {
        (true, _, _) => "-",
        (_, true, _) => "+",
        (_, _, true) => " ",
        _ => "",
    };
    match conversion {
        'd' | 'i' => format!("{}{}", sign(integer() < 0), integer().unsigned_abs()),
        'u' => (integer() as u64).to_string(),
        'x' => format!("{:x}", integer() as u64),
        'X' => format!("{:X}", integer() as u64),
        'o' => format!("{:o}", integer() as u64),
        'f' | 'F' => {
            let f = match arg {
                Value::Int(n) => *n as f64,
                Value::Float(f) => *f,
                Value::Text(text) => text.parse().unwrap_or(0.0),
            };
            format!(
                "{}{:.*}",
                sign(f < 0.0),
                spec.precision.unwrap_or(6),
                f.abs()
            )
        }
        'c' => match arg {
            Value::Text(text) => text.chars().take(1).collect(),
            _ => char::from_u32(integer() as u32)
                .map(String::from)
                .unwrap_or_default(),
        },
        _ => {
            let shown = display(arg, spec.precision.filter(|_| conversion != 's'));
            match spec.precision.filter(|_| conversion == 's') {
                Some(limit) => shown.chars().take(limit).collect(),
                None => shown,
            }
        }
    }
}

/// Pads to the width: with spaces on the left or right, or with zeros after the sign for a number.
fn pad(text: &str, spec: &Spec, conversion: char) -> String {
    let missing = spec.width.saturating_sub(text.chars().count());
    if missing == 0 {
        return text.to_string();
    }
    let numeric = "diuxXofF".contains(conversion);
    if spec.left {
        format!("{text}{}", " ".repeat(missing))
    } else if spec.zero && numeric && !(spec.precision.is_some() && "diuxXo".contains(conversion)) {
        let at = text.find(|c: char| c.is_ascii_alphanumeric()).unwrap_or(0);
        format!("{}{}{}", &text[..at], "0".repeat(missing), &text[at..])
    } else {
        format!("{}{text}", " ".repeat(missing))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Text;
    use serde_json::json;

    /// The bytes of a `.mo` file with these entries (msgid, msgstr; plural forms joined by NUL) and header.
    pub(crate) fn mo(header: &str, entries: &[(&str, &str)]) -> Vec<u8> {
        let mut all = vec![("", header)];
        all.extend_from_slice(entries);
        all.sort();
        let n = all.len() as u32;
        let (mut originals, mut translations) = (vec![], vec![]);
        let mut strings = vec![];
        let base = 28 + 16 * n;
        for (id, str) in &all {
            originals.push((id.len() as u32, base + strings.len() as u32));
            strings.extend_from_slice(id.as_bytes());
            strings.push(0);
            translations.push((str.len() as u32, base + strings.len() as u32));
            strings.extend_from_slice(str.as_bytes());
            strings.push(0);
        }
        let mut bytes = vec![];
        for word in [0x950412de_u32, 0, n, 28, 28 + 8 * n, 0, 0] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        for (len, offset) in originals.into_iter().chain(translations) {
            bytes.extend_from_slice(&len.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        bytes.extend_from_slice(&strings);
        bytes
    }

    fn header(plurals: &str) -> String {
        format!("Content-Type: text/plain; charset=UTF-8\nPlural-Forms: {plurals}\n")
    }

    fn message(value: serde_json::Value) -> LocalisedString {
        serde_json::from_value(value).unwrap()
    }

    fn catalogue(plurals: &str, entries: &[(&str, &str)]) -> Arc<Catalogue> {
        Arc::new(Catalogue::parse(&mo(&header(plurals), entries)).unwrap())
    }

    #[test]
    fn a_message_is_worded_from_the_catalogue_with_its_values_in_the_translated_order() {
        install(Some(catalogue(
            "nplurals=1; plural=0;",
            &[(
                "%s hits %s for %d.",
                "%3$d 피해: %2$s 이(가) %1$s 을(를) 쳤다.",
            )],
        )));
        let hit = message(json!({"key": "%s hits %s for %d.", "args": ["좀비", "당신", 7]}));
        assert_eq!(render(&hit), "7 피해: 당신 이(가) 좀비 을(를) 쳤다.");
    }

    #[test]
    fn a_message_without_a_translation_or_a_catalogue_shows_its_key_filled() {
        let line = message(json!({"key": "The %s dies!", "args": ["zombie"]}));
        install(None);
        assert_eq!(render(&line), "The zombie dies!");
        install(Some(catalogue(
            "nplurals=2; plural=n!=1;",
            &[("Other", "다른")],
        )));
        assert_eq!(render(&line), "The zombie dies!");
    }

    #[test]
    fn a_context_selects_its_own_entry() {
        install(Some(catalogue(
            "nplurals=1; plural=0;",
            &[
                ("verb\u{4}Fire", "발사"),
                ("noun\u{4}Fire", "불"),
                ("Fire", "화재"),
            ],
        )));
        let fire = |ctx: Option<&str>| {
            render(&message(match ctx {
                Some(ctx) => json!({"key": "Fire", "ctx": ctx}),
                None => json!({"key": "Fire"}),
            }))
        };
        assert_eq!(fire(Some("verb")), "발사");
        assert_eq!(fire(Some("noun")), "불");
        assert_eq!(fire(None), "화재");
        assert_eq!(fire(Some("other")), "Fire");
    }

    #[test]
    fn korean_has_one_plural_form_english_two_and_russian_three() {
        let rats = |count: u64| message(json!({"key": "%d rat", "plural": count, "args": [count]}));
        fn entry(forms: &str) -> [(&str, &str); 1] {
            [("%d rat", forms)]
        }
        install(Some(catalogue("nplurals=1; plural=0;", &entry("%d마리"))));
        assert_eq!(
            [0, 1, 2, 21].map(|n| render(&rats(n))),
            ["0마리", "1마리", "2마리", "21마리"]
        );
        install(Some(catalogue(
            "nplurals=2; plural=(n != 1);",
            &entry("%d rat\0%d rats"),
        )));
        assert_eq!(
            [0, 1, 2].map(|n| render(&rats(n))),
            ["0 rats", "1 rat", "2 rats"]
        );
        let russian = "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);";
        install(Some(catalogue(
            russian,
            &entry("%d крыса\0%d крысы\0%d крыс"),
        )));
        assert_eq!(
            [1, 2, 5, 11, 21, 22, 25, 111].map(|n| render(&rats(n))),
            [
                "1 крыса",
                "2 крысы",
                "5 крыс",
                "11 крыс",
                "21 крыса",
                "22 крысы",
                "25 крыс",
                "111 крыс"
            ]
        );
    }

    #[test]
    fn a_plural_message_without_a_translation_shows_its_key() {
        install(None);
        let rats = message(json!({"key": "%d rat", "plural": 3, "args": [3]}));
        assert_eq!(render(&rats), "3 rat");
    }

    #[test]
    fn positional_and_sequential_placeholders_fill_from_the_values() {
        let fill = |format: &str, args: &[Value]| printf(format, args);
        let text = |s: &str| Value::Text(s.into());
        assert_eq!(fill("%s and %s", &[text("a"), text("b")]), "a and b");
        assert_eq!(fill("%2$s and %1$s", &[text("a"), text("b")]), "b and a");
        assert_eq!(fill("%1$s %1$s", &[text("a")]), "a a");
        assert_eq!(fill("100%% of %s", &[text("x")]), "100% of x");
        assert_eq!(fill("%d%%", &[Value::Int(5)]), "5%");
        // A placeholder with no value shows nothing, and a lone percent sign ends the text.
        assert_eq!(fill("[%s][%s]", &[text("a")]), "[a][]");
        assert_eq!(fill("50%", &[]), "50");
    }

    #[test]
    fn numbers_follow_flags_width_and_precision() {
        let fill = |format: &str, arg: Value| printf(format, &[arg]);
        assert_eq!(fill("%d", Value::Int(-42)), "-42");
        assert_eq!(fill("%+d", Value::Int(42)), "+42");
        assert_eq!(fill("%5d|", Value::Int(42)), "   42|");
        assert_eq!(fill("%-5d|", Value::Int(42)), "42   |");
        assert_eq!(fill("%05d", Value::Int(-42)), "-0042");
        assert_eq!(fill("%ld", Value::Int(1 << 40)), "1099511627776");
        assert_eq!(fill("%.1f", Value::Float(2.25)), "2.2");
        assert_eq!(fill("%.1f", Value::Float(2.26)), "2.3");
        assert_eq!(fill("%f", Value::Float(1.5)), "1.500000");
        assert_eq!(fill("%6.2f|", Value::Float(-3.14259)), " -3.14|");
        assert_eq!(fill("%d", Value::Float(3.9)), "3");
        assert_eq!(fill("%f", Value::Int(2)), "2.000000");
        assert_eq!(fill("%x", Value::Int(255)), "ff");
        assert_eq!(fill("%X", Value::Int(255)), "FF");
        assert_eq!(fill("%8s|", Value::Text("한글".into())), "      한글|");
        assert_eq!(fill("%.2s", Value::Text("abc".into())), "ab");
    }

    #[test]
    fn a_text_argument_is_rendered_with_the_catalogue_too_and_the_empty_key_joins() {
        install(Some(catalogue(
            "nplurals=1; plural=0;",
            &[("zombie", "좀비")],
        )));
        let line = message(json!({"key": "The %s dies!", "args": [{"key": "zombie"}]}));
        assert_eq!(render(&line), "The 좀비 dies!");
        let joined = message(json!({"key": "", "args": [{"key": "zombie"}, " x ", 2]}));
        assert_eq!(render(&joined), "좀비 x 2");
        assert_eq!(Text::Localised(joined).shown(), "좀비 x 2");
    }

    #[test]
    fn the_catalogue_is_found_under_the_game_root_by_locale() {
        let root = std::env::temp_dir().join(format!("bn-localise-{}", std::process::id()));
        let dir = root.join("lang/mo/ko/LC_MESSAGES");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("cataclysm-bn.mo"),
            mo(&header("nplurals=1; plural=0;"), &[("Hello", "안녕")]),
        )
        .unwrap();
        let ko = Catalogue::load(&root, "ko").unwrap();
        assert_eq!(ko.entries["Hello"], ["안녕"]);
        assert!(Catalogue::load(&root, "en").is_none());
        assert!(Catalogue::load(&root.join("missing"), "ko").is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn bytes_that_are_not_a_catalogue_are_refused() {
        assert!(Catalogue::parse(b"not a catalogue").is_err());
    }

    #[test]
    fn a_plural_rule_that_is_not_an_expression_refuses_the_catalogue() {
        for rule in ["n ?", "n ? 1", "(n", "n @ 2", "1 2"] {
            let header = format!("Plural-Forms: nplurals=2; plural={rule};\n");
            assert!(Catalogue::parse(&mo(&header, &[])).is_err(), "{rule}");
        }
    }

    #[test]
    fn a_rule_applies_c_precedence_and_never_divides_by_zero() {
        let form = |rule: &str, n| Plural::parse(rule).unwrap().form(n);
        assert_eq!(form("n==0 ? 0 : n==1 ? 1 : 2", 7), 2);
        assert_eq!(form("n > 1 && n < 5 || n == 9", 9), 1);
        assert_eq!(form("!n", 0), 1);
        assert_eq!(form("1 + 2 * 3 - 4 / 2 % 5", 0), 5);
        assert_eq!(form("n % 0", 3), 0);
    }

    #[test]
    fn a_big_endian_catalogue_is_read() {
        let mut bytes = mo(&header("nplurals=1; plural=0;"), &[("Hi", "안녕")]);
        for at in (0..bytes.len().min(28 + 16 * 2)).step_by(4) {
            bytes[at..at + 4].reverse();
        }
        let le = mo(&header("nplurals=1; plural=0;"), &[("Hi", "안녕")]);
        assert_ne!(bytes, le);
        assert_eq!(Catalogue::parse(&bytes).unwrap().entries["Hi"], ["안녕"]);
    }

    #[test]
    fn the_clients_words_follow_its_catalogue_and_fall_back_to_english() {
        install_client(Some(catalogue(
            "nplurals=2; plural=(n != 1);",
            &[
                ("Cursor (%d, %d)", "%2$d, %1$d: Zeiger"),
                ("%d second", "%d Sekunde\0%d Sekunden"),
            ],
        )));
        // Through bindings, so `xgettext` does not take these test words for the client's.
        let (cursor, second, seconds) = ("Cursor (%d, %d)", "%d second", "%d seconds");
        let at = [Value::Int(3), Value::Int(4)];
        assert_eq!(tr_format(cursor, &at), "4, 3: Zeiger");
        let count = |n: u64| tr_plural(second, seconds, n, &[Value::Int(n as i64)]);
        assert_eq!([1, 2].map(count), ["1 Sekunde", "2 Sekunden"]);
        let ready = "Ready";
        assert_eq!(tr(ready), "Ready");
        install_client(None);
        assert_eq!([1, 5].map(count), ["1 second", "5 seconds"]);
        assert_eq!(tr_format(cursor, &at), "Cursor (3, 4)");
    }

    /// `tools/update_po.sh` merges every extracted message into each language, untranslated until worded.
    #[test]
    fn every_built_in_language_words_every_extracted_message() {
        let po = Path::new(env!("CARGO_MANIFEST_DIR")).join("po");
        let languages = std::fs::read_dir(&po)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "po"))
            .collect::<Vec<_>>();
        assert!(languages.len() >= 2, "{languages:?}");
        for path in languages {
            let locale = path.file_stem().unwrap().to_string_lossy().into_owned();
            assert!(
                Catalogue::client(&locale).is_some(),
                "{locale} is not built in"
            );
            for message in polib::po_file::parse(&path).unwrap().messages() {
                assert!(
                    message.is_translated() && !message.flags().is_fuzzy(),
                    "{locale} lacks {:?}",
                    message.msgid()
                );
            }
        }
        assert!(Catalogue::client("en").is_none());
        let korean = Catalogue::client("ko_KR").expect("the dialect falls back to its language");
        assert_eq!(korean.translation("Ready", None), Some("준비"));
    }
}
