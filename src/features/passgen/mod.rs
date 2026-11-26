// Password generator — pure logic, no UI.
//
// A password is drawn from up to four character classes (uppercase,
// lowercase, digits, symbols) with the process CSPRNG (`rand::rng()`, a
// ChaCha generator seeded by the OS). Rules:
//
//   * length between MIN_LENGTH and MAX_LENGTH;
//   * at least one class selected;
//   * the result always holds at least one character of every selected class;
//   * "exclude look-alike characters" removes AMBIGUOUS from every class;
//   * the symbol class is the user's own list (ASCII punctuation only).
//
// The options are part of `Settings` (keys `passgen_*`), so they are saved
// with the note and travel in a full backup.

use zeroize::Zeroizing;

pub const MIN_LENGTH: u32 = 8;
pub const MAX_LENGTH: u32 = 64;
pub const DEFAULT_LENGTH: u32 = 16;
/// Symbols offered until the user edits the list.
pub const DEFAULT_SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.?";
/// Characters easily mistaken for one another when read or typed again.
pub const AMBIGUOUS: &str = "0Oo1lI|";

const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const DIGITS: &str = "0123456789";

/// What the generator draws from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub length: u32,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
    /// Symbols allowed when `symbols` is on: distinct ASCII punctuation.
    pub symbol_set: String,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            length: DEFAULT_LENGTH,
            upper: true,
            lower: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
            symbol_set: DEFAULT_SYMBOLS.to_string(),
        }
    }
}

/// Why a password cannot be generated from some options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PassgenError {
    /// The length is not a number between MIN_LENGTH and MAX_LENGTH.
    Length,
    /// No character class is selected.
    NoClass,
    /// The symbol list holds something that is not ASCII punctuation.
    NotASymbol(char),
    /// Symbols are selected but none is left (empty list, or only look-alikes
    /// while they are excluded).
    NoSymbolLeft,
}

/// The symbol list as stored: `text` without duplicates, in typing order.
/// Anything but ASCII punctuation is refused (a letter or a digit typed there
/// would silently join another class; a space or a non-ASCII character is
/// refused by many sites).
pub fn normalize_symbols(text: &str) -> Result<String, PassgenError> {
    let mut out = String::new();
    for c in text.chars() {
        if !c.is_ascii_punctuation() {
            return Err(PassgenError::NotASymbol(c));
        }
        if !out.contains(c) {
            out.push(c);
        }
    }
    Ok(out)
}

impl Options {
    /// Options from what the generator window holds. `length` is the text of
    /// the length field.
    #[allow(clippy::too_many_arguments)]
    pub fn from_form(
        length: &str,
        upper: bool,
        lower: bool,
        digits: bool,
        symbols: bool,
        exclude_ambiguous: bool,
        symbol_set: &str,
    ) -> Result<Options, PassgenError> {
        let length = length.trim().parse::<u32>().map_err(|_| PassgenError::Length)?;
        let options = Options {
            length,
            upper,
            lower,
            digits,
            symbols,
            exclude_ambiguous,
            symbol_set: normalize_symbols(symbol_set)?,
        };
        options.classes()?;
        Ok(options)
    }

    /// The selected classes, each as the characters it may give. Fails when
    /// the options cannot produce a password.
    pub fn classes(&self) -> Result<Vec<Vec<char>>, PassgenError> {
        if !(MIN_LENGTH..=MAX_LENGTH).contains(&self.length) {
            return Err(PassgenError::Length);
        }
        // Checked even while symbols are off: the list is stored either way.
        let symbol_set = normalize_symbols(&self.symbol_set)?;
        let keep = |c: &char| !(self.exclude_ambiguous && AMBIGUOUS.contains(*c));
        let class = |chars: &str| chars.chars().filter(keep).collect::<Vec<char>>();

        let mut classes = Vec::with_capacity(4);
        if self.upper {
            classes.push(class(UPPER));
        }
        if self.lower {
            classes.push(class(LOWER));
        }
        if self.digits {
            classes.push(class(DIGITS));
        }
        if self.symbols {
            let symbols = class(&symbol_set);
            if symbols.is_empty() {
                return Err(PassgenError::NoSymbolLeft);
            }
            classes.push(symbols);
        }
        if classes.is_empty() {
            return Err(PassgenError::NoClass);
        }
        Ok(classes)
    }
}

/// Draw a password with `rng`. Every selected class is represented and every
/// position is drawn uniformly (no modulo bias).
pub fn generate_with<R: rand::Rng + ?Sized>(options: &Options, rng: &mut R) -> Result<Zeroizing<String>, PassgenError> {
    use rand::seq::SliceRandom;
    use rand::RngExt;
    use zeroize::Zeroize;

    let classes = options.classes()?;
    let all: Vec<char> = classes.iter().flatten().copied().collect();
    // MIN_LENGTH (8) is above the number of classes (4): there is room for
    // one character of each, then the rest comes from all of them together.
    let mut chars: Vec<char> = Vec::with_capacity(options.length as usize);
    for class in &classes {
        chars.push(class[rng.random_range(0..class.len())]);
    }
    while chars.len() < options.length as usize {
        chars.push(all[rng.random_range(0..all.len())]);
    }
    // The guaranteed characters must not sit at known positions.
    chars.shuffle(rng);
    let password = Zeroizing::new(chars.iter().collect::<String>());
    chars.zeroize();
    Ok(password)
}

/// Draw a password with the process CSPRNG.
pub fn generate(options: &Options) -> Result<Zeroizing<String>, PassgenError> {
    generate_with(options, &mut rand::rng())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn rng(seed: u64) -> rand::rngs::StdRng {
        rand::rngs::StdRng::seed_from_u64(seed)
    }

    fn only(upper: bool, lower: bool, digits: bool, symbols: bool) -> Options {
        Options { upper, lower, digits, symbols, ..Options::default() }
    }

    #[test]
    fn defaults_are_sixteen_characters_of_every_class() {
        let o = Options::default();
        assert_eq!(o.length, 16);
        assert!(o.upper && o.lower && o.digits && o.symbols);
        assert!(!o.exclude_ambiguous);
        assert_eq!(o.symbol_set, DEFAULT_SYMBOLS);
        assert_eq!((MIN_LENGTH, MAX_LENGTH), (8, 64));
    }

    #[test]
    fn generated_password_has_the_requested_length() {
        for length in [8, 9, 16, 33, 64] {
            let o = Options { length, ..Options::default() };
            for seed in 0..20 {
                assert_eq!(generate_with(&o, &mut rng(seed)).unwrap().chars().count(), length as usize);
            }
        }
    }

    #[test]
    fn every_selected_class_is_always_present() {
        // Shortest length, all four classes: the hardest case for the rule.
        let o = Options { length: 8, ..Options::default() };
        for seed in 0..500 {
            let p = generate_with(&o, &mut rng(seed)).unwrap();
            assert!(p.chars().any(|c| c.is_ascii_uppercase()), "seed {seed}: no uppercase in {:?}", &*p);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()), "seed {seed}: no lowercase");
            assert!(p.chars().any(|c| c.is_ascii_digit()), "seed {seed}: no digit");
            assert!(p.chars().any(|c| DEFAULT_SYMBOLS.contains(c)), "seed {seed}: no symbol");
        }
    }

    #[test]
    fn only_selected_classes_are_used() {
        for seed in 0..50 {
            let p = generate_with(&only(false, false, true, false), &mut rng(seed)).unwrap();
            assert!(p.chars().all(|c| c.is_ascii_digit()), "{:?}", &*p);
            let p = generate_with(&only(true, true, false, false), &mut rng(seed)).unwrap();
            assert!(p.chars().all(|c| c.is_ascii_alphabetic()), "{:?}", &*p);
            let p = generate_with(&only(false, false, false, true), &mut rng(seed)).unwrap();
            assert!(p.chars().all(|c| DEFAULT_SYMBOLS.contains(c)), "{:?}", &*p);
        }
    }

    #[test]
    fn custom_symbols_are_the_only_symbols() {
        let o = Options { symbol_set: "-_".into(), ..Options::default() };
        for seed in 0..100 {
            let p = generate_with(&o, &mut rng(seed)).unwrap();
            assert!(p.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "{:?}", &*p);
            assert!(p.chars().any(|c| c == '-' || c == '_'));
        }
    }

    #[test]
    fn look_alikes_are_excluded_on_request() {
        let o = Options { length: 64, exclude_ambiguous: true, symbol_set: "|!#".into(), ..Options::default() };
        for seed in 0..200 {
            let p = generate_with(&o, &mut rng(seed)).unwrap();
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)), "seed {seed}: {:?}", &*p);
        }
        // Without the option they do come out (64 characters, 200 draws).
        let o = Options { exclude_ambiguous: false, ..o };
        assert!((0..200).any(|seed| generate_with(&o, &mut rng(seed)).unwrap().chars().any(|c| AMBIGUOUS.contains(c))));
    }

    #[test]
    fn every_character_of_a_class_can_come_out() {
        // No character is unreachable (off-by-one in the draw).
        let o = Options { length: 64, ..only(false, false, true, false) };
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..50 {
            seen.extend(generate_with(&o, &mut rng(seed)).unwrap().chars());
        }
        assert_eq!(seen.into_iter().collect::<String>(), "0123456789");
    }

    #[test]
    fn two_draws_differ() {
        let o = Options::default();
        let mut r = rng(7);
        assert_ne!(generate_with(&o, &mut r).unwrap(), generate_with(&o, &mut r).unwrap());
        // And with the real generator.
        assert_ne!(generate(&o).unwrap(), generate(&o).unwrap());
    }

    #[test]
    fn impossible_options_are_refused() {
        let mut r = rng(1);
        assert_eq!(generate_with(&only(false, false, false, false), &mut r), Err(PassgenError::NoClass));
        for length in [0, 7, 65, 1000] {
            let o = Options { length, ..Options::default() };
            assert_eq!(generate_with(&o, &mut r), Err(PassgenError::Length), "length {length}");
        }
        // Symbols selected, none available.
        let o = Options { symbol_set: String::new(), ..Options::default() };
        assert_eq!(generate_with(&o, &mut r), Err(PassgenError::NoSymbolLeft));
        let o = Options { symbol_set: "|".into(), exclude_ambiguous: true, ..Options::default() };
        assert_eq!(generate_with(&o, &mut r), Err(PassgenError::NoSymbolLeft));
        // An empty list does not matter while symbols are off.
        let o = Options { symbol_set: String::new(), ..only(true, true, true, false) };
        assert!(generate_with(&o, &mut r).is_ok());
        // A list that was not normalized is refused, not silently used.
        let o = Options { symbol_set: "!a".into(), ..Options::default() };
        assert_eq!(generate_with(&o, &mut r), Err(PassgenError::NotASymbol('a')));
    }

    #[test]
    fn symbol_list_is_deduplicated_and_only_punctuation() {
        assert_eq!(normalize_symbols("!!@#@"), Ok("!@#".to_string()));
        assert_eq!(normalize_symbols(""), Ok(String::new()));
        assert_eq!(normalize_symbols(DEFAULT_SYMBOLS), Ok(DEFAULT_SYMBOLS.to_string()));
        // Every ASCII punctuation character is accepted.
        let all: String = (0x21u8..0x7F).map(char::from).filter(|c| c.is_ascii_punctuation()).collect();
        assert_eq!(all.chars().count(), 32);
        assert_eq!(normalize_symbols(&all), Ok(all.clone()));
        for (text, bad) in [("!a", 'a'), ("9#", '9'), ("# $", ' '), ("\u{e9}!", '\u{e9}'), ("!\t", '\t'), ("\u{20ac}", '\u{20ac}')] {
            assert_eq!(normalize_symbols(text), Err(PassgenError::NotASymbol(bad)), "{text:?}");
        }
    }

    #[test]
    fn form_is_validated_before_any_draw() {
        let ok = Options::from_form(" 24 ", true, false, true, true, true, "##--").unwrap();
        assert_eq!(
            ok,
            Options { length: 24, upper: true, lower: false, digits: true, symbols: true, exclude_ambiguous: true, symbol_set: "#-".into() }
        );
        for bad in ["", "abc", "7", "65", "-3", "16.5"] {
            assert_eq!(Options::from_form(bad, true, true, true, true, false, "!"), Err(PassgenError::Length), "{bad:?}");
        }
        assert_eq!(Options::from_form("16", false, false, false, false, false, "!"), Err(PassgenError::NoClass));
        assert_eq!(Options::from_form("16", true, true, true, true, false, "!x"), Err(PassgenError::NotASymbol('x')));
        assert_eq!(Options::from_form("16", true, true, true, true, false, ""), Err(PassgenError::NoSymbolLeft));
        // The symbol list is kept (and checked) even while symbols are off.
        assert_eq!(Options::from_form("16", true, false, false, false, false, "-_").unwrap().symbol_set, "-_");
        assert_eq!(Options::from_form("16", true, false, false, false, false, "é"), Err(PassgenError::NotASymbol('é')));
    }
}
