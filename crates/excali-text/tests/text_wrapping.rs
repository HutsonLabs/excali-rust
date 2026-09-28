//! `packages/element/tests/textWrapping.test.ts`, ported case for case.
//!
//! Upstream measures with jest-canvas-mock, whose `measureText(text).width`
//! is `text.length`, times 10 under `isTestEnv()`
//! (`textMeasurements.ts:142-146`): [`CharCountTextMetrics`], the test-only
//! provider (feature `test-util`). The font string is upstream's and only
//! keys the per-character width cache.
//!
//! Code points the repository's authorship gate treats as invisible
//! (zero-width joiner, variation selectors) are written as `\u{...}`
//! escapes; the strings are upstream's code point for code point.

use excali_text::text_measurements::{CharCountTextMetrics, CharWidthCache};
use excali_text::text_wrapping::{
    contains_cjk, get_wrapped_text_lines, parse_tokens, wrap_text, WrappedTextLine,
};

// textWrapping.test.ts:12
const FONT: &str = "10px Cascadia, Segoe UI Emoji";

fn wrap(text: &str, max_width: f64) -> String {
    wrap_text(
        text,
        FONT,
        max_width,
        &CharCountTextMetrics,
        &mut CharWidthCache::new(),
    )
}

fn lines(text: &str, max_width: f64) -> Vec<WrappedTextLine> {
    get_wrapped_text_lines(
        text,
        FONT,
        max_width,
        &CharCountTextMetrics,
        &mut CharWidthCache::new(),
    )
}

fn line(text: &str, start: usize, end: usize) -> WrappedTextLine {
    WrappedTextLine {
        text: text.to_owned(),
        start,
        end,
    }
}

fn tokens(text: &str) -> Vec<String> {
    parse_tokens(text)
}

// textWrapping.test.ts:14-19
#[test]
fn should_wrap_the_text_correctly_when_word_length_is_exactly_equal_to_max_width() {
    // Length of "Excalidraw" is 100 and exacty equal to max width
    assert_eq!(wrap("Hello Excalidraw", 100.0), "Hello\nExcalidraw");
}

// textWrapping.test.ts:21-26
#[test]
fn should_return_the_text_as_is_if_max_width_is_invalid() {
    let text = "Hello Excalidraw";
    assert_eq!(wrap(text, f64::NAN), text);
    assert_eq!(wrap(text, -1.0), text);
    assert_eq!(wrap(text, f64::INFINITY), text);
}

// textWrapping.test.ts:28-33
#[test]
fn should_show_the_text_correctly_when_max_width_reached() {
    assert_eq!(wrap("Hello😀", 10.0), "H\ne\nl\nl\no\n😀");
}

// textWrapping.test.ts:35-40
#[test]
fn should_not_wrap_number_when_wrapping_line() {
    assert_eq!(
        wrap("don't wrap this number 99,100.99", 300.0),
        "don't wrap this number\n99,100.99"
    );
}

// textWrapping.test.ts:42-47
#[test]
fn should_trim_all_trailing_whitespaces() {
    assert_eq!(wrap("Hello     ", 50.0), "Hello");
}

// textWrapping.test.ts:49-54
#[test]
fn should_trim_all_but_one_trailing_whitespaces() {
    assert_eq!(wrap("Hello     ", 60.0), "Hello ");
}

// textWrapping.test.ts:56-61
#[test]
fn should_keep_preceding_whitespaces_and_trim_all_trailing_whitespaces() {
    assert_eq!(wrap("  Hello  World", 90.0), "  Hello\nWorld");
}

// textWrapping.test.ts:63-68
#[test]
fn should_keep_some_preceding_whitespaces_trim_trailing_whitespaces_but_keep_those_that_fit_in_the_trailing_line(
) {
    assert_eq!(
        wrap("   Hello  World            ", 90.0),
        "   Hello\nWorld    "
    );
}

// textWrapping.test.ts:70-75
#[test]
fn should_trim_keep_those_whitespace_that_fit_in_the_trailing_line() {
    assert_eq!(
        wrap("Hello   Wo rl  d                     ", 100.0),
        "Hello   Wo\nrl  d     "
    );
}

// textWrapping.test.ts:77-82
#[test]
fn should_support_multiple_multi_codepoint_emojis() {
    let text = "😀🗺🔥👩🏽\u{200D}🦰👨\u{200D}👩\u{200D}👧\u{200D}👦🇨🇿";
    assert_eq!(
        wrap(text, 1.0),
        "😀\n🗺\n🔥\n👩🏽\u{200D}🦰\n👨\u{200D}👩\u{200D}👧\u{200D}👦\n🇨🇿"
    );
}

// textWrapping.test.ts:84-94
#[test]
fn should_wrap_the_text_correctly_when_text_contains_hyphen() {
    let text = "Wikipedia is hosted by Wikimedia- Foundation, a non-profit organization that also hosts a range-of other projects";
    assert_eq!(
        wrap(text, 110.0),
        "Wikipedia\nis hosted\nby\nWikimedia-\nFoundation,\na non-\nprofit\norganizatio\nn that also\nhosts a\nrange-of\nother\nprojects"
    );

    assert_eq!(
        wrap("Hello thereusing-now", 100.0),
        "Hello\nthereusing\n-now"
    );
}

// textWrapping.test.ts:96-106
#[test]
fn should_support_wrapping_nested_lists() {
    let text = "\tA) one tab\t\t- two tabs        - 8 spaces";
    assert_eq!(
        wrap(text, 100.0),
        "\tA) one\ntab\t\t- two\ntabs\n- 8 spaces"
    );
    assert_eq!(
        wrap(text, 50.0),
        "\tA)\none\ntab\n- two\ntabs\n- 8\nspace\ns"
    );
}

// textWrapping.test.ts:108-121
#[test]
fn should_retain_original_text_offsets_for_wrapped_lines() {
    assert_eq!(
        lines("Hello World!", 60.0),
        vec![line("Hello", 0, 5), line("World!", 6, 12)]
    );
}

// textWrapping.test.ts:123-136
#[test]
fn should_exclude_whitespace_trimmed_away_at_soft_wrap_boundaries_from_line_offsets() {
    assert_eq!(
        lines("  Hello  World", 90.0),
        vec![line("  Hello", 0, 7), line("World", 9, 14)]
    );
}

// textWrapping.test.ts:138-151
#[test]
fn should_retain_offsets_when_wrapping_a_single_long_token() {
    assert_eq!(
        lines("Excalidraw", 50.0),
        vec![line("Excal", 0, 5), line("idraw", 5, 10)]
    );
}

// textWrapping.test.ts:153-171
#[test]
fn should_preserve_empty_hard_lines_in_metadata() {
    assert_eq!(
        lines("A\n\nB", 100.0),
        vec![line("A", 0, 1), line("", 2, 2), line("B", 3, 4)]
    );
}

mod when_text_is_cjk {
    use super::*;

    // textWrapping.test.ts:174-183
    #[test]
    fn should_break_each_cjk_character_when_width_is_very_small() {
        let text = "안녕하세요こんにちは世界ｺﾝﾆﾁハ你好";
        assert_eq!(
            wrap(text, 10.0),
            "안\n녕\n하\n세\n요\nこ\nん\nに\nち\nは\n世\n界\nｺ\nﾝ\nﾆ\nﾁ\nハ\n你\n好"
        );
    }

    // textWrapping.test.ts:185-193
    #[test]
    fn should_break_cjk_text_into_longer_segments_when_width_is_larger() {
        let text = "안녕하세요こんにちは世界ｺﾝﾆﾁハ你好";
        // measureText is mocked, so it's not precisely what would happen in prod
        assert_eq!(
            wrap(text, 30.0),
            "안녕하\n세요こ\nんにち\nは世界\nｺﾝﾆ\nﾁハ你\n好"
        );
    }

    // textWrapping.test.ts:195-209
    #[test]
    fn should_handle_a_combination_of_cjk_latin_emojis_and_whitespaces() {
        let text = "a醫 醫      bb  你好  world-i-😀🗺🔥";
        assert_eq!(wrap(text, 150.0), "a醫 醫      bb  你\n好  world-i-😀🗺\n🔥");
        assert_eq!(wrap(text, 50.0), "a醫 醫\nbb  你\n好\nworld\n-i-😀\n🗺🔥");
        assert_eq!(
            wrap(text, 30.0),
            "a醫\n醫\nbb\n你好\nwor\nld-\ni-\n😀\n🗺\n🔥"
        );
    }

    // textWrapping.test.ts:211-220
    #[test]
    fn should_break_before_and_after_a_regular_cjk_character() {
        let text = "HelloたWorld";
        assert_eq!(wrap(text, 50.0), "Hello\nた\nWorld");
        assert_eq!(wrap(text, 60.0), "Helloた\nWorld");
    }

    // textWrapping.test.ts:222-231
    #[test]
    fn should_break_before_and_after_certain_cjk_symbols() {
        let text = "こんにちは〃世界";
        assert_eq!(wrap(text, 50.0), "こんにちは\n〃世界");
        assert_eq!(wrap(text, 60.0), "こんにちは〃\n世界");
    }

    // textWrapping.test.ts:233-238
    #[test]
    fn should_break_after_not_before_for_certain_cjk_pairs() {
        assert_eq!(wrap("Hello た。", 70.0), "Hello\nた。");
    }

    // textWrapping.test.ts:240-245
    #[test]
    fn should_break_before_not_after_for_certain_cjk_pairs() {
        assert_eq!(wrap("Hello「たWorld」", 60.0), "Hello\n「た\nWorld」");
    }

    // textWrapping.test.ts:247-252
    #[test]
    fn should_break_after_not_before_for_certain_cjk_character_pairs() {
        assert_eq!(wrap("「Helloた」World", 70.0), "「Hello\nた」World");
    }

    // textWrapping.test.ts:254-270
    #[test]
    fn should_break_chinese_sentences() {
        let text = "中国你好！这是一个测试。\n我们来看看：人民币¥1234「很贵」\n（括号）、逗号，句号。空格 换行　全角符号…—";
        assert_eq!(
            wrap(text, 80.0),
            "中国你好！这是一\n个测试。\n我们来看看：人民\n币¥1234「很\n贵」\n（括号）、逗号，\n句号。空格 换行\n全角符号…—"
        );
        assert_eq!(
            wrap(text, 50.0),
            "中国你好！\n这是一个测\n试。\n我们来看\n看：人民币\n¥1234\n「很贵」\n（括号）、\n逗号，句\n号。空格\n换行　全角\n符号…—"
        );
    }

    // textWrapping.test.ts:272-291
    #[test]
    fn should_break_japanese_sentences() {
        let text = "日本こんにちは！これはテストです。\n  見てみましょう：円￥1234「高い」\n  （括弧）、読点、句点。\n  空白 改行　全角記号…ー";
        assert_eq!(
            wrap(text, 80.0),
            "日本こんにちは！\nこれはテストで\nす。\n  見てみましょ\nう：円￥1234\n「高い」\n  （括弧）、読\n点、句点。\n  空白 改行\n全角記号…ー"
        );
        assert_eq!(
            wrap(text, 50.0),
            "日本こんに\nちは！これ\nはテストで\nす。\n  見てみ\nましょう：\n円\n￥1234\n「高い」\n  （括\n弧）、読\n点、句点。\n  空白\n改行　全角\n記号…ー"
        );
    }

    // textWrapping.test.ts:293-312
    #[test]
    fn should_break_korean_sentences() {
        let text = "한국 안녕하세요! 이것은 테스트입니다.\n우리 보자: 원화₩1234「비싸다」\n(괄호), 쉼표, 마침표.\n공백 줄바꿈　전각기호…—";
        assert_eq!(
            wrap(text, 80.0),
            "한국 안녕하세\n요! 이것은 테\n스트입니다.\n우리 보자: 원\n화₩1234「비\n싸다」\n(괄호), 쉼\n표, 마침표.\n공백 줄바꿈　전\n각기호…—"
        );
        assert_eq!(
            wrap(text, 60.0),
            "한국 안녕하\n세요! 이것\n은 테스트입\n니다.\n우리 보자:\n원화\n₩1234\n「비싸다」\n(괄호),\n쉼표, 마침\n표.\n공백 줄바꿈\n전각기호…—"
        );
    }
}

mod when_text_contains_leading_whitespaces {
    use super::*;

    // textWrapping.test.ts:316
    const TEXT: &str = "  \t   Hello world";

    // textWrapping.test.ts:318-322
    #[test]
    fn should_preserve_leading_whitespaces() {
        assert_eq!(wrap(TEXT, 120.0), "  \t   Hello\nworld");
    }

    // textWrapping.test.ts:324-328
    #[test]
    fn should_break_and_collapse_leading_whitespaces_when_line_breaks() {
        assert_eq!(wrap(TEXT, 60.0), "\nHello\nworld");
    }

    // textWrapping.test.ts:330-334
    #[test]
    fn should_break_and_collapse_leading_whitespaces_whe_words_break() {
        assert_eq!(wrap(TEXT, 30.0), "\nHel\nlo\nwor\nld");
    }
}

mod when_text_contains_trailing_whitespaces {
    use super::*;

    // textWrapping.test.ts:338-343
    #[test]
    fn shouldnt_add_new_lines_for_trailing_spaces() {
        let text = "Hello whats up     ";
        assert_eq!(wrap(text, 190.0), text);
    }

    // textWrapping.test.ts:345-350
    #[test]
    fn should_ignore_trailing_whitespaces_when_line_breaks() {
        assert_eq!(
            wrap("Hippopotomonstrosesquippedaliophobia        ??????", 400.0),
            "Hippopotomonstrosesquippedaliophobia\n??????"
        );
    }

    // textWrapping.test.ts:352-357
    #[test]
    fn should_not_ignore_trailing_whitespaces_when_word_breaks() {
        assert_eq!(
            wrap("Hippopotomonstrosesquippedaliophobia        ??????", 300.0),
            "Hippopotomonstrosesquippedalio\nphobia        ??????"
        );
    }

    // textWrapping.test.ts:359-364
    #[test]
    fn should_ignore_trailing_whitespaces_when_word_breaks_and_line_breaks() {
        assert_eq!(
            wrap("Hippopotomonstrosesquippedaliophobia        ??????", 180.0),
            "Hippopotomonstrose\nsquippedaliophobia\n??????"
        );
    }
}

// textWrapping.test.ts:367-402
#[test]
fn when_text_doesnt_contain_new_lines() {
    let text = "Hello whats up";
    let cases: [(&str, f64, &str); 5] = [
        (
            "break all words when width of each word is less than container width",
            70.0,
            "Hello\nwhats\nup",
        ),
        (
            "break all characters when width of each character is less than container width",
            15.0,
            "H\ne\nl\nl\no\nw\nh\na\nt\ns\nu\np",
        ),
        ("break words as per the width", 130.0, "Hello whats\nup"),
        ("fit the container", 240.0, "Hello whats up"),
        (
            "push the word if its equal to max width",
            50.0,
            "Hello\nwhats\nup",
        ),
    ];
    for (desc, width, res) in cases {
        assert_eq!(wrap(text, width), res, "should {desc}");
    }
}

// textWrapping.test.ts:404-429
#[test]
fn when_text_contain_new_lines() {
    let text = "Hello\n  whats up";
    let cases: [(&str, f64, &str); 3] = [
        (
            "break all words when width of each word is less than container width",
            70.0,
            "Hello\n  whats\nup",
        ),
        (
            "break all characters when width of each character is less than container width",
            15.0,
            "H\ne\nl\nl\no\n\nw\nh\na\nt\ns\nu\np",
        ),
        ("break words as per the width", 140.0, "Hello\n  whats up"),
    ];
    for (desc, width, res) in cases {
        assert_eq!(
            wrap(text, width),
            res,
            "should respect new lines and {desc}"
        );
    }
}

// textWrapping.test.ts:431-459
#[test]
fn when_text_is_long() {
    let text = "hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg break it now";
    let cases: [(&str, f64, &str); 3] = [
        (
            "fit characters of long string as per container width",
            160.0,
            "hellolongtextthi\nsiswhatsupwithyo\nuIamtypingggggan\ndtypinggg break\nit now",
        ),
        (
            "fit characters of long string as per container width and break words as per the width",
            120.0,
            "hellolongtex\ntthisiswhats\nupwithyouIam\ntypingggggan\ndtypinggg\nbreak it now",
        ),
        (
            "fit the long text when container width is greater than text length and move the rest to next line",
            590.0,
            "hellolongtextthisiswhatsupwithyouIamtypingggggandtypinggg\nbreak it now",
        ),
    ];
    for (desc, width, res) in cases {
        assert_eq!(wrap(text, width), res, "should {desc}");
    }
}

mod test_parse_tokens {
    use super::*;

    // textWrapping.test.ts:462-515
    #[test]
    fn should_tokenize_latin() {
        assert_eq!(
            tokens("Excalidraw is a virtual collaborative whiteboard"),
            [
                "Excalidraw",
                " ",
                "is",
                " ",
                "a",
                " ",
                "virtual",
                " ",
                "collaborative",
                " ",
                "whiteboard",
            ]
        );

        assert_eq!(
            tokens("Wikipedia is hosted by Wikimedia- Foundation, a non-profit organization that also hosts a range-of other projects"),
            [
                "Wikipedia",
                " ",
                "is",
                " ",
                "hosted",
                " ",
                "by",
                " ",
                "Wikimedia-",
                " ",
                "Foundation,",
                " ",
                "a",
                " ",
                "non-",
                "profit",
                " ",
                "organization",
                " ",
                "that",
                " ",
                "also",
                " ",
                "hosts",
                " ",
                "a",
                " ",
                "range-",
                "of",
                " ",
                "other",
                " ",
                "projects",
            ]
        );
    }

    // textWrapping.test.ts:517-521
    #[test]
    fn should_not_tokenize_number() {
        assert_eq!(tokens("99,100.99"), ["99,100.99"]);
    }

    // textWrapping.test.ts:523-545
    #[test]
    fn should_tokenize_joined_emojis() {
        let text = "😬🌍🗺🔥☂\u{FE0F}👩🏽\u{200D}🦰👨\u{200D}👩\u{200D}👧\u{200D}👦👩🏾\u{200D}🔬🏳\u{FE0F}\u{200D}🌈🧔\u{200D}♀\u{FE0F}🧑\u{200D}🤝\u{200D}🧑🙅🏽\u{200D}♂\u{FE0F}✅0\u{FE0F}⃣🇨🇿🦅";
        assert_eq!(
            tokens(text),
            [
                "😬",
                "🌍",
                "🗺",
                "🔥",
                "☂\u{FE0F}",
                "👩🏽\u{200D}🦰",
                "👨\u{200D}👩\u{200D}👧\u{200D}👦",
                "👩🏾\u{200D}🔬",
                "🏳\u{FE0F}\u{200D}🌈",
                "🧔\u{200D}♀\u{FE0F}",
                "🧑\u{200D}🤝\u{200D}🧑",
                "🙅🏽\u{200D}♂\u{FE0F}",
                "✅",
                "0\u{FE0F}⃣",
                "🇨🇿",
                "🦅",
            ]
        );
    }

    // textWrapping.test.ts:547-583
    #[test]
    fn should_tokenize_emojis_mixed_with_mixed_text() {
        let text = "😬a🌍b🗺c🔥d☂\u{FE0F}《👩🏽\u{200D}🦰》👨\u{200D}👩\u{200D}👧\u{200D}👦德👩🏾\u{200D}🔬こ🏳\u{FE0F}\u{200D}🌈안🧔\u{200D}♀\u{FE0F}g🧑\u{200D}🤝\u{200D}🧑h🙅🏽\u{200D}♂\u{FE0F}e✅f0\u{FE0F}⃣g🇨🇿10🦅#hash";
        assert_eq!(
            tokens(text),
            [
                "😬",
                "a",
                "🌍",
                "b",
                "🗺",
                "c",
                "🔥",
                "d",
                "☂\u{FE0F}",
                "《",
                "👩🏽\u{200D}🦰",
                "》",
                "👨\u{200D}👩\u{200D}👧\u{200D}👦",
                "德",
                "👩🏾\u{200D}🔬",
                "こ",
                "🏳\u{FE0F}\u{200D}🌈",
                "안",
                "🧔\u{200D}♀\u{FE0F}",
                "g",
                "🧑\u{200D}🤝\u{200D}🧑",
                "h",
                "🙅🏽\u{200D}♂\u{FE0F}",
                "e",
                "✅",
                // bummer, but ok, as we traded kecaps not breaking (less common) for hash and numbers not breaking (more common)
                "f0\u{FE0F}⃣g",
                "🇨🇿",
                // nice! do not break the number, as it's by default matched by \p{Emoji}
                "10",
                "🦅",
                // nice! do not break the hash, as it's by default matched by \p{Emoji}
                "#hash",
            ]
        );
    }

    // textWrapping.test.ts:585-594
    #[test]
    fn should_tokenize_decomposed_chars_into_their_composed_variants() {
        // each input character is in a decomposed form
        let text = "c\u{30C}\u{3066}\u{3099}a\u{308}\u{3072}\u{309A}\u{3B5}\u{301}\u{1103}\u{1161}\u{438}\u{306}\u{1112}\u{1161}\u{11AB}";
        let nfc = icu_normalizer::ComposingNormalizerBorrowed::new_nfc().normalize(text);
        assert_eq!(nfc.encode_utf16().count(), 8);
        assert_eq!(
            icu_normalizer::DecomposingNormalizerBorrowed::new_nfd().normalize(text),
            text
        );

        let tokens = tokens(text);
        assert_eq!(tokens.len(), 8);
        assert_eq!(tokens, ["č", "で", "ä", "ぴ", "έ", "다", "й", "한"]);
    }

    // textWrapping.test.ts:596-701
    #[test]
    fn should_tokenize_artificial_cjk() {
        let text = "《道德經》醫-醫こんにちは世界！안녕하세요세계；요』,다.다...원/달(((다)))[[1]]〚({((한))>)〛(「た」)た…[Hello] \t　World？ニューヨーク・￥3700.55す。090-1234-5678￥1,000〜＄5,000「素晴らしい！」〔重要〕＃１：Taro君30％は、（たなばた）〰￥110±￥570で20℃〜9:30〜10:00【一番】";
        let tokens = tokens(text);

        // The whole list upstream records in the test's comment
        // (textWrapping.test.ts:598-617); the assertions below are
        // upstream's `toContain` checks.
        assert_eq!(
            tokens,
            [
                "《道",
                "德",
                "經》",
                "醫-",
                "醫",
                "こ",
                "ん",
                "に",
                "ち",
                "は",
                "世",
                "界！",
                "안",
                "녕",
                "하",
                "세",
                "요",
                "세",
                "계；",
                "요』,",
                "다.",
                "다...",
                "원/",
                "달",
                "(((다)))",
                "[[1]]",
                "〚({((한))>)〛",
                "(「た」)",
                "た…",
                "[Hello]",
                " ",
                "\t",
                "　",
                "World？",
                "ニ",
                "ュ",
                "ー",
                "ヨ",
                "ー",
                "ク・",
                "￥3700.55",
                "す。",
                "090-",
                "1234-",
                "5678",
                "￥1,000〜",
                "＄5,000",
                "「素",
                "晴",
                "ら",
                "し",
                "い！」",
                "〔重",
                "要〕",
                "＃",
                "１：",
                "Taro",
                "君",
                "30％",
                "は、",
                "（た",
                "な",
                "ば",
                "た）",
                "〰",
                "￥110±",
                "￥570",
                "で",
                "20℃〜",
                "9:30〜",
                "10:00",
                "【一",
                "番】",
            ]
        );

        let contains = |t: &str| assert!(tokens.iter().any(|x| x == t), "{t:?} not in {tokens:?}");

        // Latin
        for t in ["[[1]]", "[Hello]", "World？", "Taro"] {
            contains(t);
        }

        // Chinese
        for t in ["《道", "德", "經》", "醫-", "醫"] {
            contains(t);
        }

        // Japanese
        for t in [
            "こ",
            "ん",
            "に",
            "ち",
            "は",
            "世",
            "ク・",
            "界！",
            "た…",
            "す。",
            "ュ",
            "「素",
            "晴",
            "ら",
            "し",
            "い！」",
            "君",
            "は、",
            "（た",
            "な",
            "ば",
            "た）",
            "で",
            "【一",
            "番】",
        ] {
            contains(t);
        }

        // Check for Korean
        for t in [
            "안",
            "녕",
            "하",
            "세",
            "요",
            "세",
            "계；",
            "요』,",
            "다.",
            "다...",
            "원/",
            "달",
            "(((다)))",
            "〚({((한))>)〛",
            "(「た」)",
        ] {
            contains(t);
        }

        // Numbers and units
        for t in [
            "￥3700.55",
            "090-",
            "1234-",
            "5678",
            "￥1,000〜",
            "＄5,000",
            "１：",
            "30％",
            "￥110±",
            "20℃〜",
            "9:30〜",
            "10:00",
        ] {
            contains(t);
        }

        // Punctuation and symbols
        for t in [" ", "\t", "　", "ニ", "ー", "ヨ", "〰", "＃"] {
            contains(t);
        }
    }
}

// Beyond textWrapping.test.ts: the module's other export and edge cases of
// the ported functions that upstream's code decides but its tests do not
// pin. tests/text_wrapping_goldens.rs holds the port to upstream's output
// on a larger corpus.

/// `containsCJK` (`textWrapping.ts:30-36`) is exported from the wrapping
/// module as upstream does.
#[test]
fn contains_cjk_is_exported() {
    assert!(contains_cjk("Hello 你好"));
    assert!(contains_cjk("ー"));
    assert!(!contains_cjk("Hello"));
    assert!(!contains_cjk(""));
}

/// `"".split(regex).filter(Boolean)` is `[]`.
#[test]
fn parse_tokens_of_empty_line_is_empty() {
    assert!(parse_tokens("").is_empty());
}

/// Offsets are UTF-16 code units, as upstream's `start`/`end` are
/// (`textWrapping.ts:409-418`): an astral emoji counts two.
#[test]
fn offsets_are_utf16_code_units() {
    assert_eq!(
        lines("😀 ab", 30.0),
        vec![line("😀", 0, 2), line("ab", 3, 5)]
    );
}

/// An invalid width keeps the hard lines with their offsets
/// (`getHardLineBreaks`, `textWrapping.ts:423-437`).
#[test]
fn invalid_width_keeps_hard_lines_with_offsets() {
    for width in [f64::NAN, -1.0, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            lines("ab\n\ncd e", width),
            vec![line("ab", 0, 2), line("", 3, 3), line("cd e", 4, 8)]
        );
    }
}

/// JS `\s` is not Unicode `White_Space`: U+FEFF is a space and U+0085 is
/// not (ECMA-262 WhiteSpace and LineTerminator).
#[test]
fn js_whitespace_is_used_for_breaks() {
    assert_eq!(tokens("a\u{FEFF}b"), ["a", "\u{FEFF}", "b"]);
    assert_eq!(tokens("a\u{85}b"), ["a\u{85}b"]);
    assert_eq!(
        tokens("a\u{A0}b\u{3000}c"),
        ["a", "\u{A0}", "b", "\u{3000}", "c"]
    );
}

/// `trimLine`'s `/^(.+?)(\s+)$/` needs one character before the trailing
/// run, so an all-space trailing line keeps its first space even when it
/// does not fit (`line.trimEnd()` would keep none).
#[test]
fn all_whitespace_trailing_line_keeps_its_first_space() {
    assert_eq!(wrap("     ", 5.0), " ");
    assert_eq!(wrap("     ", 20.0), "  ");
    assert_eq!(wrap("ab     ", 10.0), "a\nb");
}
