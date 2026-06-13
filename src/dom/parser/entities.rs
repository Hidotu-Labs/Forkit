/// Decode HTML entities and numeric character references in `s`.
///
/// Iterates over the string using byte positions for entity scanning but
/// always copies non-entity text as complete UTF-8 character slices so that
/// multi-byte characters (Turkish, Arabic, CJK, emoji, …) are preserved.
pub fn decode_entities(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;   // byte position in `s`

    while i < bytes.len() {
        if bytes[i] != b'&' {
            let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }

        let start = i;
        let end = bytes[i+1..].iter().take(32).position(|&b| b == b';')
            .map(|p| i + 1 + p + 1);

        if let Some(end) = end {
            let entity = &s[start..end];

            if let Some(inner) = entity.strip_prefix("&#") {
                let inner = inner.trim_end_matches(';');
                let code: Option<u32> = if inner.starts_with('x') || inner.starts_with('X') {
                    u32::from_str_radix(&inner[1..], 16).ok()
                } else {
                    inner.parse::<u32>().ok()
                };
                if let Some(cp) = code.and_then(char::from_u32) {
                    out.push(cp);
                    i = end;
                    continue;
                }
            }

            let name = entity.trim_start_matches('&').trim_end_matches(';');
            if let Some(ch) = named_entity(name) {
                out.push(ch);
                i = end;
                continue;
            }
        }

        out.push('&');
        i += 1;
    }
    out
}

pub fn named_entity(name: &str) -> Option<char> {
    Some(match name {
        // Core
        "amp"     => '&',  "lt"       => '<',  "gt"      => '>',
        "quot"    => '"',  "apos"     => '\'', "nbsp"    => '\u{00A0}',
        // Symbols
        "copy"    => '©',  "reg"      => '®',  "trade"   => '™',
        "mdash"   => '—',  "ndash"    => '–',  "hellip"  => '…',
        "laquo"   => '«',  "raquo"    => '»',
        "lsquo"   => '\u{2018}', "rsquo" => '\u{2019}',
        "ldquo"   => '\u{201C}', "rdquo" => '\u{201D}',
        "bull"    => '•',  "middot"   => '·',  "dagger"  => '†',
        "Dagger"  => '‡',  "permil"   => '‰',  "prime"   => '′',
        "Prime"   => '″',  "euro"     => '€',  "pound"   => '£',
        "yen"     => '¥',  "cent"     => '¢',  "curren"  => '¤',
        "deg"     => '°',  "plusmn"   => '±',  "times"   => '×',
        "divide"  => '÷',  "frac14"   => '¼',  "frac12"  => '½',
        "frac34"  => '¾',  "sup1"     => '¹',  "sup2"    => '²',
        "sup3"    => '³',  "micro"    => 'µ',  "para"    => '¶',
        "sect"    => '§',  "iexcl"    => '¡',  "iquest"  => '¿',
        "acute"   => '´',  "cedil"    => '¸',  "uml"     => '¨',
        "macr"    => '¯',  "ordf"     => 'ª',  "ordm"    => 'º',
        "szlig"   => 'ß',  "thorn"    => 'þ',  "Thorn"   => 'Þ',
        "eth"     => 'ð',  "ETH"      => 'Ð',
        // Latin extended
        "agrave"  => 'à', "Agrave"  => 'À', "aacute"  => 'á', "Aacute" => 'Á',
        "acirc"   => 'â', "Acirc"   => 'Â', "atilde"  => 'ã', "Atilde" => 'Ã',
        "auml"    => 'ä', "Auml"    => 'Ä', "aring"   => 'å', "Aring"  => 'Å',
        "aelig"   => 'æ', "AElig"   => 'Æ',
        "egrave"  => 'è', "Egrave"  => 'È', "eacute"  => 'é', "Eacute" => 'É',
        "ecirc"   => 'ê', "Ecirc"   => 'Ê', "euml"    => 'ë', "Euml"   => 'Ë',
        "igrave"  => 'ì', "Igrave"  => 'Ì', "iacute"  => 'í', "Iacute" => 'Í',
        "icirc"   => 'î', "Icirc"   => 'Î', "iuml"    => 'ï', "Iuml"   => 'Ï',
        "ograve"  => 'ò', "Ograve"  => 'Ò', "oacute"  => 'ó', "Oacute" => 'Ó',
        "ocirc"   => 'ô', "Ocirc"   => 'Ô', "otilde"  => 'õ', "Otilde" => 'Õ',
        "ouml"    => 'ö', "Ouml"    => 'Ö', "oslash"  => 'ø', "Oslash" => 'Ø',
        "ugrave"  => 'ù', "Ugrave"  => 'Ù', "uacute"  => 'ú', "Uacute" => 'Ú',
        "ucirc"   => 'û', "Ucirc"   => 'Û', "uuml"    => 'ü', "Uuml"   => 'Ü',
        "yacute"  => 'ý', "Yacute"  => 'Ý', "yuml"    => 'ÿ',
        "ccedil"  => 'ç', "Ccedil"  => 'Ç', "ntilde"  => 'ñ', "Ntilde" => 'Ñ',
        // Turkish-specific Latin Extended-A (HTML5 names)
        "Gbreve"  => 'Ğ', "gbreve"  => 'ğ',   // G-breve (Ğ ğ)
        "Idot"    => 'İ', "inodot"  => 'ı',   // dotted I / dotless i (İ ı)
        "Scedil"  => 'Ş', "scedil"  => 'ş',   // S-cedilla (Ş ş)
        // Other Latin Extended-A not yet listed
        "Umacr"   => 'Ū', "umacr"   => 'ū',
        "Amacr"   => 'Ā', "amacr"   => 'ā',
        "Emacr"   => 'Ē', "emacr"   => 'ē',
        "Imacr"   => 'Ī', "imacr"   => 'ī',
        "Omacr"   => 'Ō', "omacr"   => 'ō',
        // Greek
        "alpha"   => 'α', "beta"    => 'β', "gamma"   => 'γ', "delta"  => 'δ',
        "epsilon" => 'ε', "zeta"    => 'ζ', "eta"     => 'η', "theta"  => 'θ',
        "iota"    => 'ι', "kappa"   => 'κ', "lambda"  => 'λ', "mu"     => 'μ',
        "nu"      => 'ν', "xi"      => 'ξ', "pi"      => 'π', "rho"    => 'ρ',
        "sigma"   => 'σ', "tau"     => 'τ', "upsilon" => 'υ', "phi"    => 'φ',
        "chi"     => 'χ', "psi"     => 'ψ', "omega"   => 'ω',
        // Card suits & misc symbols
        "spades"  => '♠', "clubs"   => '♣', "hearts"  => '♥', "diams"  => '♦',
        "larr"    => '←', "uarr"    => '↑', "rarr"    => '→', "darr"   => '↓',
        "harr"    => '↔', "crarr"   => '↵',
        // Math
        "forall"  => '∀', "exist"   => '∃', "empty"   => '∅', "nabla"  => '∇',
        "isin"    => '∈', "notin"   => '∉', "ni"      => '∋', "prod"   => '∏',
        "sum"     => '∑', "minus"   => '−', "lowast"  => '∗', "radic"  => '√',
        "infin"   => '∞', "ang"     => '∠', "and"     => '∧', "or"     => '∨',
        "cap"     => '∩', "cup"     => '∪', "int"     => '∫', "there4" => '∴',
        "sim"     => '∼', "cong"    => '≅', "asymp"   => '≈', "ne"     => '≠',
        "equiv"   => '≡', "le"      => '≤', "ge"      => '≥', "sub"    => '⊂',
        "sup"     => '⊃', "sube"    => '⊆', "supe"    => '⊇', "oplus"  => '⊕',
        "otimes"  => '⊗', "perp"    => '⊥', "sdot"    => '⋅',
        "loz"     => '◊',
        // Spaces / zero-width
        "ensp"    => '\u{2002}', "emsp"   => '\u{2003}',
        "thinsp"  => '\u{2009}', "zwnj"   => '\u{200C}',
        "zwj"     => '\u{200D}', "lrm"    => '\u{200E}',
        "rlm"     => '\u{200F}',
        _ => return None,
    })
}
