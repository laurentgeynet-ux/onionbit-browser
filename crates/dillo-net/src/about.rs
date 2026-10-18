//! Port of `src/IO/about.c` — page de démarrage `about:splash`.
//!
//! `VERSION` amont = `3.3.0-rc1` (config.h du checkout de référence).

/// `AboutSplash` — HTML verbatim de l'écran d'accueil.
pub const ABOUT_SPLASH: &str = "<!DOCTYPE HTML>\n\
<html>\n\
<head>\n\
  <title>Quickstart</title>\n\
  <style>\n\
    body {\n\
      background: white;\n\
      margin: 3em;\n\
      font-size: 16px;\n\
      font-family: sans-serif;\n\
      line-height: 1.4em;\n\
    }\n\
    .main { max-width: 40em; }\n\
    p { margin-top: 1em; }\n\
    ul { margin-left: 1em; }\n\
    li { margin-top: 0.5em; }\n\
    kbd {\n\
      display: inline-block;\n\
      border: solid 1px #999;\n\
      padding: 1px 3px;\n\
    }\n\
  </style>\n\
</head>\n\
<body>\n\
<div class=\"main\">\n\
\n\
<h1>Quickstart</h1>\n\
\n\
<p>Welcome to OnionBit-Browser (a Rust port of Dillo 3.3.0-rc1), a small and fast\n\
graphical web browser. To access the help click the question mark button\n\
<code><kbd>?</kbd></code> in the top right corner at any time. Here are some tips to get you started:</p>\n\
\n\
<ul>\n\
  <li>The main configuration file is at <code>~/.dillo/dillorc</code>.</li>\n\
  <li>Most actions can also be done by using the <a href='about:keys'>keyboard</a>.</li>\n\
  <li>Cookies are <em>disabled by default</em>.</li>\n\
  <li>Several OnionBit-Browser plugins are available.</li>\n\
</ul>\n\
\n\
<p>See more details in the\n\
<a href=\"https://dillo-browser.org/\">Dillo website</a>.</p>\n\
\n\
</div>\n\
</body>\n\
</html>\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splash_is_verbatim() {
        // points de contrôle structurels du document amont
        assert!(ABOUT_SPLASH.starts_with("<!DOCTYPE HTML>\n<html>\n"));
        assert!(ABOUT_SPLASH.ends_with("</body>\n</html>\n"));
        assert!(ABOUT_SPLASH.contains("Welcome to OnionBit-Browser"));
        assert!(ABOUT_SPLASH.contains("Dillo 3.3.0-rc1"));
        assert!(ABOUT_SPLASH.contains("href='about:keys'"));
        assert!(ABOUT_SPLASH.contains("https://dillo-browser.org/"));
        // 45 lignes terminées par '\n' comme la concaténation amont
        assert_eq!(ABOUT_SPLASH.lines().count(), 45);
    }
}
