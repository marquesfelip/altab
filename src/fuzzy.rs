//! Pesquisa fuzzy (subsequência com ranking, estilo fzf) sobre apps e janelas.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::windows_list::AppEntry;

/// Resultado de um app que casou com a consulta.
pub struct Match {
    /// Índice em `entries`.
    pub app: usize,
    /// Índice da janela do app a ser exibida/ativada.
    pub window: usize,
    /// Posições (em `char`) do nome do app que casaram, para destaque.
    pub name_hits: Vec<u32>,
}

/// O nome do app pesa mais que o título de uma janela.
const NAME_BOOST_NUM: u32 = 3;
const NAME_BOOST_DEN: u32 = 2;

/// Filtra e ordena `apps` por relevância. Consulta vazia mantém todos na
/// ordem original (uso recente primeiro); empates também preservam a ordem.
pub fn rank(query: &str, apps: &[AppEntry]) -> Vec<Match> {
    if query.trim().is_empty() {
        return (0..apps.len())
            .map(|app| Match { app, window: 0, name_hits: Vec::new() })
            .collect();
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, Match)> = Vec::new();

    for (app, entry) in apps.iter().enumerate() {
        let mut hits = Vec::new();
        let name_score = pattern
            .indices(Utf32Str::new(&entry.name, &mut buf), &mut matcher, &mut hits)
            .map(|s| s * NAME_BOOST_NUM / NAME_BOOST_DEN);

        // Melhor título; em empate vence a janela mais recente (menor índice).
        let mut best_title: Option<(u32, usize)> = None;
        for (i, w) in entry.windows.iter().enumerate() {
            if let Some(s) = pattern.score(Utf32Str::new(&w.title, &mut buf), &mut matcher) {
                if best_title.is_none_or(|(b, _)| s > b) {
                    best_title = Some((s, i));
                }
            }
        }

        let (score, window) = match (name_score, best_title) {
            (None, None) => continue,
            (Some(n), None) => (n, 0),
            (None, Some((t, i))) => (t, i),
            (Some(n), Some((t, i))) => {
                if n >= t {
                    (n, 0)
                } else {
                    (t, i)
                }
            }
        };

        hits.sort_unstable();
        hits.dedup();
        let name_hits = if name_score.is_some() { hits } else { Vec::new() };
        scored.push((score, Match { app, window, name_hits }));
    }

    // `sort_by` é estável: empates mantêm a ordem de uso recente.
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_list::WinInfo;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::HICON;

    fn app(name: &str, titles: &[&str]) -> AppEntry {
        AppEntry {
            name: name.to_string(),
            icon: HICON::default(),
            windows: titles
                .iter()
                .map(|t| WinInfo { hwnd: HWND::default(), title: t.to_string() })
                .collect(),
        }
    }

    fn names(q: &str, apps: &[AppEntry]) -> Vec<String> {
        rank(q, apps).iter().map(|m| apps[m.app].name.clone()).collect()
    }

    fn sample() -> Vec<AppEntry> {
        vec![
            app("Windows Terminal", &["PowerShell"]),
            app("Google Chrome", &["Projeto Arquitetura - Google Chrome", "Gmail"]),
            app("Visual Studio Code", &["main.rs - altab"]),
            app("Opções do altab", &["Opções do altab"]),
        ]
    }

    #[test]
    fn empty_query_keeps_order() {
        let apps = sample();
        assert_eq!(
            names("", &apps),
            ["Windows Terminal", "Google Chrome", "Visual Studio Code", "Opções do altab"]
        );
    }

    #[test]
    fn subsequence_and_initials() {
        let apps = sample();
        assert_eq!(names("chr", &apps)[0], "Google Chrome");
        assert_eq!(names("vsc", &apps), ["Visual Studio Code"]);
    }

    #[test]
    fn ignores_accents_and_case() {
        let apps = sample();
        assert_eq!(names("OPCOES", &apps), ["Opções do altab"]);
    }

    #[test]
    fn multiple_terms_are_anded() {
        let apps = sample();
        assert_eq!(names("code vis", &apps), ["Visual Studio Code"]);
        assert!(names("code chrome", &apps).is_empty());
    }

    #[test]
    fn no_match_is_empty() {
        assert!(names("zzzz", &sample()).is_empty());
    }

    #[test]
    fn title_match_selects_that_window() {
        let apps = sample();
        let r = rank("gmail", &apps);
        assert_eq!(apps[r[0].app].name, "Google Chrome");
        assert_eq!(r[0].window, 1);
        assert!(r[0].name_hits.is_empty());
    }

    #[test]
    fn name_hits_are_sorted_char_positions() {
        let apps = sample();
        let r = rank("vsc", &apps);
        assert!(r[0].name_hits.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(r[0].name_hits.len(), 3);
    }

    #[test]
    fn ties_keep_recency_order() {
        let apps = vec![app("Notes", &["a"]), app("Notes", &["b"])];
        let r = rank("notes", &apps);
        assert_eq!(r.iter().map(|m| m.app).collect::<Vec<_>>(), [0, 1]);
    }
}
