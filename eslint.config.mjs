import tsPlugin from '@typescript-eslint/eslint-plugin'
import tsParser from '@typescript-eslint/parser'
import prettier from 'eslint-config-prettier'

async function optionalImport(packageName) {
  try {
    return (await import(packageName)).default
  } catch {
    return null
  }
}

const angular = await optionalImport('@angular-eslint/eslint-plugin')
const angularTemplate = await optionalImport('@angular-eslint/eslint-plugin-template')
const angularTemplateParser = await optionalImport('@angular-eslint/template-parser')

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
      ...(angular ? { '@angular-eslint': angular } : {}),
    },
    rules: {
      ...tsPlugin.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      '@typescript-eslint/explicit-function-return-type': 'warn',
      'no-console': 'off',
    },
  },
  prettier,
]

if (angularTemplate && angularTemplateParser) {
  config.splice(2, 0, {
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
  })
}

export default config
