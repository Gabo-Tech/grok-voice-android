# Agents

UI and theme work follows [DESIGN-LANGUAGE.md](DESIGN-LANGUAGE.md). Cursor also loads `.cursor/rules/design-language.mdc`.

Read that file before changing a screen. Themes only override the semantic tokens it lists. Do not add raw colors, outlined cards, corner brackets, or a second accent in a component.

Copy for the interface exists in English, Spanish, German, Portuguese, and Valencian. Apostrophes in Android `strings.xml` are written as `\'`.

Voice CRUD parity is required. Cursor also loads `.cursor/rules/voice-crud.mdc`.

Every entity the person can see must be created, read, updated, and deleted from the screen and by Talk. Deletes wait for a clear yes. Talk does not create an account, card, or other record they did not ask for. A new field or a new kind of record ships with all four operations on both paths, in en, es, de, pt, and ca-ES-valencia.
