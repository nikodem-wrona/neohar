import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type * as Preset from '@docusaurus/preset-classic';
import type { Config } from '@docusaurus/types';
import { themes as prismThemes } from 'prism-react-renderer';

const repo = 'https://github.com/nikodem-wrona/neohar';

/**
 * The "Edit this page" link of a page. A generated page has none: it starts with the note that
 * `scripts/lib.ts` writes, and its source is elsewhere in the repository.
 */
function editUrl({ versionDocsDirPath, docPath }: { versionDocsDirPath: string; docPath: string }) {
  const text = readFileSync(join(__dirname, versionDocsDirPath, docPath), 'utf8');
  if (text.includes(':::info[Generated page]')) return undefined;
  return `${repo}/edit/main/website/${versionDocsDirPath}/${docPath}`;
}

const config: Config = {
  title: 'neohar',
  tagline:
    'A native desktop workspace for supervising coding agents. The Rust core does the work; the webview paints.',
  favicon: 'img/icon.png',

  url: process.env.DOCS_URL ?? 'https://nikodem-wrona.github.io',
  baseUrl: process.env.DOCS_BASE_URL ?? '/neohar/',
  organizationName: 'nikodem-wrona',
  projectName: 'neohar',
  trailingSlash: false,

  onBrokenLinks: process.env.DOCS_STRICT ? 'throw' : 'warn',
  onBrokenAnchors: 'warn',

  i18n: { defaultLocale: 'en', locales: ['en'] },

  markdown: {
    // `.md` files are CommonMark and `.mdx` files are MDX.
    format: 'detect',
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  themes: [
    '@docusaurus/theme-mermaid',
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexBlog: false,
        docsRouteBasePath: ['docs'],
        docsPluginIdForPreferredVersion: 'default',
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
        searchResultLimits: 12,
      },
    ],
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          path: 'docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl,
          showLastUpdateTime: false,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/icon.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    docs: {
      sidebar: { hideable: true, autoCollapseCategories: true },
    },
    tableOfContents: { minHeadingLevel: 2, maxHeadingLevel: 4 },
    mermaid: {
      theme: { light: 'neutral', dark: 'dark' },
    },
    navbar: {
      title: 'neohar',
      logo: { alt: 'neohar', src: 'img/icon.png' },
      hideOnScroll: false,
      items: [
        { type: 'docSidebar', sidebarId: 'guide', position: 'left', label: 'Guide' },
        { type: 'docSidebar', sidebarId: 'api', position: 'left', label: 'API' },
        { type: 'docSidebar', sidebarId: 'crates', position: 'left', label: 'Code' },
        { href: repo, label: 'GitHub', position: 'right' },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Learn',
          items: [
            { label: 'Introduction', to: '/docs/intro' },
            { label: 'Architecture', to: '/docs/architecture/overview' },
            { label: 'Agents', to: '/docs/agents/overview' },
          ],
        },
        {
          title: 'Reference',
          items: [
            { label: 'API contract', to: '/docs/reference/api' },
            { label: 'CLI', to: '/docs/reference/cli' },
            { label: 'Configuration', to: '/docs/reference/configuration' },
          ],
        },
        {
          title: 'Repository',
          items: [{ label: 'GitHub', href: repo }],
        },
      ],
      copyright: 'Apache-2.0. neohar is not affiliated with Anthropic or OpenAI.',
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.vsDark,
      additionalLanguages: ['rust', 'toml', 'bash', 'json', 'diff', 'ini'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
