# Translating sysorbit

sysorbit's interface text lives in [Fluent](https://projectfluent.org/) files,
one per language:

```
i18n/en/sysorbit.ftl   English, the source language
i18n/tr/sysorbit.ftl   Turkish
```

The files are embedded in the binary at build time, so adding a language
needs no code change.

## Adding a language

1. Copy `i18n/en/sysorbit.ftl` to `i18n/<code>/sysorbit.ftl`, where `<code>`
   is the language's code such as `de` or `pt-BR`.
2. Translate the text after each `=`. Keep the message ids on the left and
   the `{ $variables }` as they are; you may move variables around.
3. Try it: `SYSORBIT_LANG=<code> cargo run`.
4. Run `cargo test`. It fails if a file does not parse or uses an id the
   English file does not have.
5. Open a pull request.

A message you leave out is shown in English, so a partial translation is
fine to start with.

## What not to translate

- systemd's own words: `OnCalendar=`, `Persistent=`, `WakeSystem=`,
  `AccuracySec=`, `RandomizedDelaySec=`, unit names, unit file states shown
  as code.
- Values in systemd's notation, e.g. `AccuracySec=12h`.

## Grammar

- **Plurals** use selectors on numbers, following your language's plural
  rules:

  ```
  span-day = { $n ->
      [one] 1 day
     *[other] { $n } days
  }
  ```

  A language without plural forms can write `span-day = { $n } gün`.

- **Ordinals** get `$category`, the number's ordinal plural category in your
  language (`one`, `two`, `few`, `many`, `other`), next to `$n`.

- **Sentences about schedules** (`explain-*`) are assembled from pieces. The
  messages near the end of that section (`explain-daily`, `explain-dated`,
  `explain-repeat-dated`, `explain-repeat-window`, `explain-day-of`) decide
  the order of the pieces, so you can reorder them for your language.

- **Suffixes on numbers.** Times and numbers arrive already formatted. If
  your language attaches suffixes to them that depend on how the number is
  pronounced (Turkish "03:00'te", "1'inde"), prefer phrasings that attach
  the suffix to a fixed word instead: "saat 03:00", "1. günü", "30 geçe".

## Language selection

sysorbit follows the desktop's locale (`LANGUAGE`, `LC_ALL`, `LC_MESSAGES`,
`LANG`). `SYSORBIT_LANG` overrides it.
