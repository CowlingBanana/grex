/*
 * Copyright © 2019-2020 Peter M. Stahl pemistahl@gmail.com
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either expressed or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use crate::char::ColorizableString;
use crate::regexp::RegExpConfig;
use colored::ColoredString;
use itertools::Itertools;
use std::cmp::{max, min, Ordering};
use std::fmt::{Display, Formatter, Result};

const CHARS_TO_ESCAPE: [&str; 14] = [
    "(", ")", "[", "]", "{", "}", "+", "*", "-", ".", "?", "|", "^", "$",
];

#[derive(PartialEq, Eq, Debug)]
pub(crate) enum GraphemeOverlapState {
    Left,
    Right,
    Overlap,
}

#[derive(Clone, Debug, Hash, Ord, PartialOrd, Eq, PartialEq)]
pub struct Grapheme {
    pub(crate) chars: Vec<String>,
    pub(crate) repetitions: Vec<Grapheme>,
    min: u32,
    max: u32,
    config: RegExpConfig,
}

impl Grapheme {
    pub(crate) fn from(s: &str, config: &RegExpConfig) -> Self {
        Self {
            chars: vec![s.to_string()],
            repetitions: vec![],
            min: 1,
            max: 1,
            config: config.clone(),
        }
    }

    pub(crate) fn new(chars: Vec<String>, min: u32, max: u32, config: &RegExpConfig) -> Self {
        Self {
            chars,
            repetitions: vec![],
            min,
            max,
            config: config.clone(),
        }
    }

    pub(crate) fn value(&self) -> String {
        self.chars.join("")
    }

    pub(crate) fn chars(&self) -> &Vec<String> {
        &self.chars
    }

    pub(crate) fn chars_mut(&mut self) -> &mut Vec<String> {
        &mut self.chars
    }

    pub(crate) fn has_repetitions(&self) -> bool {
        !self.repetitions.is_empty()
    }

    pub(crate) fn repetitions_mut(&mut self) -> &mut Vec<Grapheme> {
        &mut self.repetitions
    }

    pub(crate) fn minimum(&self) -> u32 {
        self.min
    }

    pub(crate) fn maximum(&self) -> u32 {
        self.max
    }

    pub(crate) fn char_count(&self, is_non_ascii_char_escaped: bool) -> usize {
        if is_non_ascii_char_escaped {
            self.chars
                .iter()
                .map(|it| it.chars().map(|c| self.escape(c, false)).join(""))
                .join("")
                .chars()
                .count()
        } else {
            self.chars.iter().map(|it| it.chars().count()).sum()
        }
    }

    pub(crate) fn escape_non_ascii_chars(&mut self, use_surrogate_pairs: bool) {
        self.chars = self
            .chars
            .iter()
            .map(|it| {
                it.chars()
                    .map(|c| self.escape(c, use_surrogate_pairs))
                    .join("")
            })
            .collect_vec();
    }

    pub(crate) fn escape_regexp_symbols(
        &mut self,
        is_non_ascii_char_escaped: bool,
        is_astral_code_point_converted_to_surrogate: bool,
    ) {
        let characters = self.chars_mut();

        #[allow(clippy::needless_range_loop)]
        for i in 0..characters.len() {
            let mut character = characters[i].clone();

            for char_to_escape in CHARS_TO_ESCAPE.iter() {
                character =
                    character.replace(char_to_escape, &format!("{}{}", "\\", char_to_escape));
            }

            character = character
                .replace("\n", "\\n")
                .replace("\r", "\\r")
                .replace("\t", "\\t");

            if character == "\\" {
                character = "\\\\".to_string();
            }

            characters[i] = character;
        }

        if is_non_ascii_char_escaped {
            self.escape_non_ascii_chars(is_astral_code_point_converted_to_surrogate);
        }
    }

    fn escape(&self, c: char, use_surrogate_pairs: bool) -> String {
        if c.is_ascii() {
            c.to_string()
        } else if use_surrogate_pairs && ('\u{10000}'..'\u{10ffff}').contains(&c) {
            self.convert_to_surrogate_pair(c)
        } else {
            c.escape_unicode().to_string()
        }
    }

    fn convert_to_surrogate_pair(&self, c: char) -> String {
        c.encode_utf16(&mut [0; 2])
            .iter()
            .map(|it| format!("\\u{{{:x}}}", it))
            .join("")
    }

    pub(crate) fn overlap_with(&self, other: &Self) -> Option<Vec<(Self, GraphemeOverlapState)>> {
        if self.chars != other.chars {
            return None;
        }

        if self.min > other.min || (self.min == other.min && self.max > other.max) {
            return Some(
                other
                    .overlap_with(self)?
                    .iter()
                    .map(|(g, state)| (g.clone(), state.flip()))
                    .collect_vec(),
            );
        }

        let mut result = Vec::new();
        if self.min < other.min {
            result.push((
                Self::new(
                    self.chars.clone(),
                    self.min,
                    min(self.max, other.min - 1),
                    &self.config,
                ),
                GraphemeOverlapState::Left,
            ));
        }

        if self.max >= other.min {
            result.push((
                Self::new(
                    self.chars.clone(),
                    other.min,
                    min(self.max, other.max),
                    &self.config,
                ),
                GraphemeOverlapState::Overlap,
            ));
        }

        match self.max.cmp(&other.max) {
            Ordering::Less => result.push((
                Self::new(
                    self.chars.clone(),
                    max(self.max + 1, other.min),
                    other.max,
                    &self.config,
                ),
                GraphemeOverlapState::Right,
            )),
            Ordering::Equal => (),
            Ordering::Greater => result.push((
                Self::new(
                    self.chars.clone(),
                    max(other.max + 1, self.min),
                    self.max,
                    &self.config,
                ),
                GraphemeOverlapState::Left,
            )),
        }

        Some(result)
    }
}

impl GraphemeOverlapState {
    fn flip(&self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Overlap => Self::Overlap,
        }
    }
}

impl Display for Grapheme {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let is_single_char = self.char_count(false) == 1
            || (self.chars.len() == 1 && self.chars[0].matches('\\').count() == 1);
        let is_range = self.min < self.max;
        let is_repetition = self.min > 1;
        let value = if self.repetitions.is_empty() {
            self.value()
        } else {
            self.repetitions.iter().map(|it| it.to_string()).join("")
        };

        let (
            colored_value,
            comma,
            left_brace,
            right_brace,
            left_parenthesis,
            right_parenthesis,
            min,
            max,
        ) = to_colorized_string(
            vec![
                ColorizableString::from(&value),
                ColorizableString::Comma,
                ColorizableString::LeftBrace,
                ColorizableString::RightBrace,
                if self.config.is_capturing_group_enabled() {
                    ColorizableString::CapturingLeftParenthesis
                } else {
                    ColorizableString::NonCapturingLeftParenthesis
                },
                ColorizableString::RightParenthesis,
                ColorizableString::Number(self.min),
                ColorizableString::Number(self.max),
            ],
            &self.config,
        );

        if !is_range && is_repetition && is_single_char {
            write!(f, "{}{}{}{}", colored_value, left_brace, min, right_brace)
        } else if !is_range && is_repetition && !is_single_char {
            write!(
                f,
                "{}{}{}{}{}{}",
                left_parenthesis, colored_value, right_parenthesis, left_brace, min, right_brace
            )
        } else if is_range && is_single_char {
            write!(
                f,
                "{}{}{}{}{}{}",
                colored_value, left_brace, min, comma, max, right_brace
            )
        } else if is_range && !is_single_char {
            write!(
                f,
                "{}{}{}{}{}{}{}{}",
                left_parenthesis,
                colored_value,
                right_parenthesis,
                left_brace,
                min,
                comma,
                max,
                right_brace
            )
        } else {
            write!(f, "{}", colored_value)
        }
    }
}

fn to_colorized_string(
    strings: Vec<ColorizableString>,
    config: &RegExpConfig,
) -> (
    ColoredString,
    ColoredString,
    ColoredString,
    ColoredString,
    ColoredString,
    ColoredString,
    ColoredString,
    ColoredString,
) {
    let v = strings
        .iter()
        .map(|it| it.to_colorized_string(config.is_output_colorized))
        .collect_vec();

    (
        v[0].clone(),
        v[1].clone(),
        v[2].clone(),
        v[3].clone(),
        v[4].clone(),
        v[5].clone(),
        v[6].clone(),
        v[7].clone(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlap_same() {
        let config = RegExpConfig::new();
        let chars = vec![String::from("a")];

        let grapheme1 = Grapheme::new(chars, 0, 0, &config);

        assert_eq!(
            grapheme1.overlap_with(&grapheme1),
            Some(vec![(grapheme1, GraphemeOverlapState::Overlap),])
        );
    }

    #[test]
    fn test_overlap_disjoint() {
        let config = RegExpConfig::new();
        let chars = vec![String::from("a")];

        let grapheme1 = Grapheme::new(chars.clone(), 0, 0, &config);
        let grapheme2 = Grapheme::new(chars, 10, 10, &config);

        assert_eq!(
            grapheme2.overlap_with(&grapheme1),
            Some(vec![
                (grapheme1, GraphemeOverlapState::Right),
                (grapheme2, GraphemeOverlapState::Left)
            ])
        );
    }

    #[test]
    fn test_overlap_initial_match() {
        let config = RegExpConfig::new();
        let chars = vec![String::from("a")];

        let grapheme1 = Grapheme::new(chars.clone(), 0, 0, &config);
        let grapheme2 = Grapheme::new(chars.clone(), 0, 1, &config);

        assert_eq!(
            grapheme1.overlap_with(&grapheme2),
            Some(vec![
                (grapheme1, GraphemeOverlapState::Overlap),
                (
                    Grapheme::new(chars, 1, 1, &config),
                    GraphemeOverlapState::Right
                )
            ])
        );
    }

    #[test]
    fn test_overlap_initial_non_match() {
        let config = RegExpConfig::new();
        let chars = vec![String::from("a")];

        let grapheme1 = Grapheme::new(chars.clone(), 0, 10, &config);
        let grapheme2 = Grapheme::new(chars.clone(), 5, 15, &config);

        assert_eq!(
            grapheme1.overlap_with(&grapheme2),
            Some(vec![
                (
                    Grapheme::new(chars.clone(), 0, 4, &config),
                    GraphemeOverlapState::Left
                ),
                (
                    Grapheme::new(chars.clone(), 5, 10, &config),
                    GraphemeOverlapState::Overlap
                ),
                (
                    Grapheme::new(chars, 11, 15, &config),
                    GraphemeOverlapState::Right
                ),
            ])
        );
    }

    #[test]
    fn test_overlap_fully_contained() {
        let config = RegExpConfig::new();
        let chars = vec![String::from("a")];

        let grapheme1 = Grapheme::new(chars.clone(), 0, 15, &config);
        let grapheme2 = Grapheme::new(chars.clone(), 5, 10, &config);

        assert_eq!(
            grapheme1.overlap_with(&grapheme2),
            Some(vec![
                (
                    Grapheme::new(chars.clone(), 0, 4, &config),
                    GraphemeOverlapState::Left
                ),
                (
                    Grapheme::new(chars.clone(), 5, 10, &config),
                    GraphemeOverlapState::Overlap
                ),
                (
                    Grapheme::new(chars, 11, 15, &config),
                    GraphemeOverlapState::Left
                ),
            ])
        );
    }
}
