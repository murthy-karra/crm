import js from '@eslint/js'
import vue from 'eslint-plugin-vue'
import tseslint from 'typescript-eslint'
import globals from 'globals'

export default tseslint.config(
  { ignores: ['dist/**', 'node_modules/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs['flat/recommended'],
  {
    files: ['**/*.vue'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      // Single-word names for atomic design-system primitives (matching
      // the vocabulary docs/design/UI_STYLE.md itself uses: "Card",
      // "Badge") — a sanctioned, common exception to the multi-word rule,
      // not a blanket opt-out.
      'vue/multi-word-component-names': ['error', { ignores: ['Card', 'Badge'] }],
    },
  },
  {
    languageOptions: {
      globals: {
        ...globals.browser,
        ...globals.node,
      },
    },
  },
)
