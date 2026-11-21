// Every plugin is a plain import: a missing one fails the lint instead of leaving its rules out.
import angular from '@angular-eslint/eslint-plugin'
import angularTemplate from '@angular-eslint/eslint-plugin-template'
import angularTemplateParser from '@angular-eslint/template-parser'
import tsPlugin from '@typescript-eslint/eslint-plugin'
import tsParser from '@typescript-eslint/parser'
import prettier from 'eslint-config-prettier'

/**
 * Code that decides on a `.message` (`x.message`, `x?.message`, `x['message']`): comparing it,
 * switching on it, testing or searching it. Showing it (concatenation, templates) is fine.
 * `scripts/eslint-message-rule.test.mjs` pins what is caught and what is not.
 */
function messageDecisions() {
  const message = [
    "MemberExpression[computed=false][property.name='message']",
    "MemberExpression[computed=true][property.value='message']",
  ].flatMap(member => [member, `ChainExpression > ${member}`])
  const at = position => message.map(member => `${position} > ${member}`)
  const selectors = [
    ...at('BinaryExpression[operator=/^[!=]==?$/]'),
    ...message.map(member =>
      member.startsWith('ChainExpression > ')
        ? `SwitchStatement > ChainExpression.discriminant > ${member.slice('ChainExpression > '.length)}`
        : `SwitchStatement > ${member}.discriminant`
    ),
    ...at('CallExpression[callee.property.name=/^(test|exec)$/]'),
    // A list searched for the message: `['busy'].includes(e.message)`.
    ...at(
      'CallExpression[callee.property.name=/^(includes|indexOf|lastIndexOf|some|find|findIndex|has)$/]'
    ),
    ...message.map(
      member =>
        `CallExpression > MemberExpression.callee[property.name=/^(includes|startsWith|endsWith|match|matchAll|indexOf|lastIndexOf|search|localeCompare)$/] > ${member.replace('ChainExpression > ', '')}.object`
    ),
  ]
  return [...new Set(selectors)].map(selector => ({
    selector,
    message: 'Decide on a failure’s kind, never on its message.',
  }))
}

const config = [
  {
    ignores: [
      'api/**',
      'cli/**',
      'core/**',
      '.direnv/**',
      'desktop/**',
      'dashboard/dist/**',
      'dist/**',
      'node_modules/**',
      'target/**',
    ],
  },
  {
    files: ['**/*.ts'],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: 2022,
        sourceType: 'module',
      },
    },
    plugins: {
      '@typescript-eslint': tsPlugin,
      '@angular-eslint': angular,
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/explicit-function-return-type': 'warn',
      'no-console': 'off',
    },
  },
  {
    files: ['dashboard/src/**/*.html'],
    languageOptions: {
      parser: angularTemplateParser,
    },
    plugins: {
      '@angular-eslint/template': angularTemplate,
    },
    rules: {
      '@angular-eslint/template/click-events-have-key-events': 'error',
      '@angular-eslint/template/interactive-supports-focus': 'error',
      '@angular-eslint/template/label-has-associated-control': 'error',
      '@angular-eslint/template/mouse-events-have-key-events': 'error',
    },
  },
  {
    // Failures are told apart by their kind; a message is for people (AGENTS.md).
    files: ['dashboard/src/app/**/*.ts'],
    ignores: ['dashboard/src/app/**/*.spec.ts'],
    rules: { 'no-restricted-syntax': ['error', ...messageDecisions()] },
  },
  prettier,
]

export default config
