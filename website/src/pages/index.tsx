import Link from '@docusaurus/Link';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import type { ReactNode } from 'react';
import styles from './index.module.css';

interface Agent {
  name: string;
  harness: string;
  state: string;
  tone: 'work' | 'wait' | 'idle' | 'done';
  source: string;
}

// A still picture of the sidebar of the app: each run with its state and where the state came from.
const agents: Agent[] = [
  { name: 'w1 · api', harness: 'claude', state: 'working', tone: 'work', source: 'hook' },
  { name: 'w2 · web', harness: 'claude', state: 'approval', tone: 'wait', source: 'hook' },
  { name: 'w3 · infra', harness: 'claude', state: 'done', tone: 'done', source: 'transcript' },
  { name: 'w4 · docs', harness: 'codex', state: 'idle', tone: 'idle', source: 'screen ~' },
];

function Supervisor(): ReactNode {
  return (
    <div className={styles.window} aria-hidden="true">
      <div className={styles.titlebar}>
        <span className={styles.dot} />
        <span className={styles.dot} />
        <span className={styles.dot} />
        <span className={styles.titletext}>neohar — default</span>
      </div>
      <div className={styles.body}>
        <div className={styles.sidebar}>
          <div className={styles.sideHead}>Agents</div>
          {agents.map((a) => (
            <div key={a.name} className={styles.agent}>
              <span className={`${styles.badge} ${styles[a.tone]}`} />
              <span className={styles.agentName}>{a.name}</span>
              <span className={styles.agentMeta}>
                {a.harness} · {a.state}
              </span>
              <span className={styles.agentSource}>{a.source}</span>
            </div>
          ))}
        </div>
        <div className={styles.pane}>
          <div className={styles.paneStrip}>w1 · api — claude · focused</div>
          <div className={styles.term}>
            <div>
              <span className={styles.prompt}>~/api $</span> claude
            </div>
            <div className={styles.dim}>⏺ Reading src/routes/orders.ts</div>
            <div className={styles.dim}>⏺ Updating the handler and its tests</div>
            <div className={styles.dim}>✻ Working… (esc to interrupt)</div>
            <div>
              <span className={styles.cursor} />
            </div>
          </div>
        </div>
        <div className={styles.drawer}>
          <div className={styles.sideHead}>Inbox · 1</div>
          <div className={styles.card}>
            <div className={styles.cardHead}>
              <span className={styles.cardKind}>Approval · i42</span>
              <span className={styles.risk}>risk: medium</span>
            </div>
            <div className={styles.cardWhy}>w2 · web wants to run</div>
            <code className={styles.cmd}>rm -rf build &amp;&amp; bun run build</code>
            <div className={styles.actions}>
              <span className={styles.allow}>Allow</span>
              <span className={styles.always}>Always</span>
              <span className={styles.deny}>Deny</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

interface Bet {
  title: string;
  text: string;
  to: string;
}

const bets: Bet[] = [
  {
    title: 'Processes outlive the app',
    text: 'One nh hold process per pane owns the PTY and journals output with safe cut points. Quit, crash or update the app: agents keep running, and screens come back exactly.',
    to: '/docs/architecture/holders',
  },
  {
    title: 'Structured first, screen last',
    text: 'Agent state comes from hooks, protocol streams and transcripts. Every facet carries its source and confidence; reading the screen is the last fallback.',
    to: '/docs/agents/overview',
  },
  {
    title: 'Interactions are objects',
    text: 'Approvals, questions and plan reviews have ids, risk and a delivery state machine. An answer from a card, a notification or the CLI goes through one recoverable transaction.',
    to: '/docs/agents/interactions',
  },
  {
    title: 'The webview paints, it does not think',
    text: 'VT parsing, damage, selection and encoding happen in Rust. The UI gets binary frames, decodes them in WASM and draws them with WebGL2. Lint rules enforce the boundary.',
    to: '/docs/desktop/overview',
  },
];

interface Door {
  title: string;
  text: string;
  to: string;
}

const doors: Door[] = [
  {
    title: 'Install locally',
    text: 'Build, install, connect the agents',
    to: '/docs/guides/install',
  },
  {
    title: 'Architecture',
    text: 'Processes, core, rendering, storage',
    to: '/docs/architecture/overview',
  },
  { title: 'Agents', text: 'Claude Code, Codex, gating, delivery', to: '/docs/agents/overview' },
  {
    title: 'Desktop and UI',
    text: 'Tauri host, transport, shell, surface',
    to: '/docs/desktop/overview',
  },
  {
    title: 'API reference',
    text: 'Every method, projection, event, type',
    to: '/docs/reference/api',
  },
  { title: 'Command line', text: 'Every nh command', to: '/docs/reference/cli' },
  { title: 'Code', text: 'The map of crates, packages and tools', to: '/docs/code' },
  { title: 'Testing', text: 'Checks, tests and local suites', to: '/docs/testing/overview' },
];

export default function Home(): ReactNode {
  const icon = useBaseUrl('/img/icon.png');
  return (
    <Layout
      title="A desktop workspace for supervising coding agents"
      description="neohar: a Rust core owns terminals, agent state and approvals, and keeps agent processes alive across app quit, crash and update."
    >
      <header className={styles.hero}>
        <div className={styles.heroInner}>
          <div className={styles.heroText}>
            <img src={icon} alt="" className={styles.logo} width={56} height={56} />
            <h1 className={styles.title}>neohar</h1>
            <p className={styles.tagline}>
              A native desktop workspace for supervising coding agents. A Rust core owns the
              terminals, the agents&apos; state and their approvals, and keeps every agent process
              alive through anything short of a reboot. The window paints.
            </p>
            <p className={styles.harnesses}>
              For <strong>Claude Code</strong> and <strong>Codex</strong>. macOS first. Tauri 2,
              Rust and SolidJS.
            </p>
            <div className={styles.buttons}>
              <Link className="button button--primary button--lg" to="/docs/intro">
                Read the introduction
              </Link>
              <Link
                className="button button--secondary button--lg"
                to="/docs/architecture/overview"
              >
                Architecture
              </Link>
              <Link className={styles.textLink} to="/docs/guides/install">
                Install it locally →
              </Link>
            </div>
          </div>
          <Supervisor />
        </div>
      </header>

      <main>
        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>What neohar bets on</h2>
          <div className={styles.bets}>
            {bets.map((b) => (
              <Link key={b.title} to={b.to} className={styles.bet}>
                <h3>{b.title}</h3>
                <p>{b.text}</p>
              </Link>
            ))}
          </div>
        </section>

        <section className={styles.section}>
          <h2 className={styles.sectionTitle}>Find your way</h2>
          <div className={styles.doors}>
            {doors.map((d) => (
              <Link key={d.title} to={d.to} className={styles.door}>
                <span className={styles.doorTitle}>{d.title}</span>
                <span className={styles.doorText}>{d.text}</span>
              </Link>
            ))}
          </div>
        </section>
      </main>
    </Layout>
  );
}
