# Agents

UI and theme work follows [DESIGN-LANGUAGE.md](DESIGN-LANGUAGE.md) v2. Cursor also loads `.cursor/rules/design-language.mdc`.

Read that file before changing a screen. Themes only override the semantic tokens it lists. v2 allows cards, the 4/8/12/20 radius scale, hero figures, and charts. It keeps the restrained dark look, one metal accent, and the bans on glass, gradients, emoji, raw colors, and a second accent.

Pull request #132 (Talk bubbles, the landscape side rail, Activity, Sankey, and Plan) lands before the later visual phases. v2 builds on it and does not replace it.

- Talk keeps the bubbles. Restyle them later with v2 tokens: a `--surface-2` assistant bubble, an accent-tinted user bubble, the shared radius token, and `.t-body`. The voice orb and its states sit at the bottom above the input, replacing the ring panel. The live caption is the streaming assistant bubble.
- The landscape rail is the Material navigation rail for the same five destinations as the portrait bar (Talk, Notes, Agenda, Finance, Settings), with the same icons, active indicator, and order.
- Finance keeps that review’s Activity, Sankey, and Plan. The Overview “Money flow” preview card opens that Sankey full screen. Plan and Activity take the v2 card and list row. The hero net worth figure and trend go on top of Overview.
- If that review clashes with a v2 token or the 48px target, keep its structure and fix the styling.

Copy for the interface exists in English, Spanish, German, Portuguese, and Valencian. Apostrophes in Android `strings.xml` are written as `\'`.

Voice CRUD parity is required. Cursor also loads `.cursor/rules/voice-crud.mdc`.

Every entity the person can see must be created, read, updated, and deleted from the screen and by Talk. Deletes wait for a clear yes. Talk does not create an account, card, or other record they did not ask for. A new field or a new kind of record ships with all four operations on both paths, in en, es, de, pt, and ca-ES-valencia.
