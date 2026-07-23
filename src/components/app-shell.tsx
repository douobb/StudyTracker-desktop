import {
  CalendarDotsIcon,
  ChartLineUpIcon,
  CheckCircleIcon,
  ClockCountdownIcon,
  GearSixIcon,
  MoonIcon,
  ShieldCheckIcon,
  SunIcon,
  TargetIcon,
} from "@phosphor-icons/react";
import * as Tooltip from "@radix-ui/react-tooltip";
import type { ReactNode } from "react";
import type { FoundationStatus } from "../platform/foundation";

interface AppShellProps {
  status: FoundationStatus | null;
  error: string | null;
  theme: "light" | "dark";
  onToggleTheme: () => void;
}

const navigation: readonly {
  label: string;
  icon: typeof CalendarDotsIcon;
  current: boolean;
}[] = [
  { label: "今日計畫", icon: CalendarDotsIcon, current: true },
  { label: "專注計時", icon: ClockCountdownIcon, current: false },
  { label: "學習統計", icon: ChartLineUpIcon, current: false },
  { label: "規則與隱私", icon: ShieldCheckIcon, current: false },
  { label: "設定", icon: GearSixIcon, current: false },
];

function StatusPill({ children }: { children: ReactNode }) {
  return (
    <span className="status-pill">
      <span className="status-dot" aria-hidden="true" />
      {children}
    </span>
  );
}

export function AppShell({
  status,
  error,
  theme,
  onToggleTheme,
}: AppShellProps) {
  const ThemeIcon = theme === "dark" ? SunIcon : MoonIcon;
  const themeLabel = theme === "dark" ? "切換為亮色模式" : "切換為暗色模式";

  return (
    <Tooltip.Provider delayDuration={400}>
      <a className="skip-link" href="#main-content">
        跳至主要內容
      </a>
      <div className="app-frame">
        <aside className="sidebar" aria-label="主要導覽">
          <div className="brand">
            <span className="brand-mark" aria-hidden="true">
              <TargetIcon weight="duotone" />
            </span>
            <span>
              <strong>StudyTracker</strong>
              <small>專注，留下可信紀錄</small>
            </span>
          </div>

          <nav aria-label="功能">
            <ul className="nav-list">
              {navigation.map(({ label, icon: Icon, current }) => (
                <li key={label}>
                  <button
                    className="nav-item"
                    type="button"
                    aria-current={current ? "page" : undefined}
                  >
                    <Icon aria-hidden="true" />
                    <span>{label}</span>
                  </button>
                </li>
              ))}
            </ul>
          </nav>

          <div className="sidebar-footer">
            <p>資料只保存在這台裝置</p>
            <StatusPill>離線模式</StatusPill>
          </div>
        </aside>

        <main id="main-content" className="main-content" tabIndex={-1}>
          <header className="topbar">
            <div>
              <p className="eyebrow">工作階段 1・工程基線</p>
              <h1>今天想完成什麼？</h1>
            </div>
            <Tooltip.Root>
              <Tooltip.Trigger asChild>
                <button
                  className="icon-button"
                  type="button"
                  onClick={onToggleTheme}
                  aria-label={themeLabel}
                >
                  <ThemeIcon aria-hidden="true" />
                </button>
              </Tooltip.Trigger>
              <Tooltip.Portal>
                <Tooltip.Content className="tooltip" sideOffset={8}>
                  {themeLabel}
                  <Tooltip.Arrow className="tooltip-arrow" />
                </Tooltip.Content>
              </Tooltip.Portal>
            </Tooltip.Root>
          </header>

          <section className="hero-card" aria-labelledby="focus-heading">
            <div>
              <p className="eyebrow">下一個專注時段</p>
              <h2 id="focus-heading">從一個清楚的小目標開始</h2>
              <p>
                選擇科目與計時方式後開始。StudyTracker
                會在本機保存進度，不需要帳號或網路。
              </p>
            </div>
            <button className="primary-button" type="button">
              <ClockCountdownIcon aria-hidden="true" />
              開始專注
            </button>
          </section>

          <section className="content-grid" aria-label="今日摘要">
            <article className="panel">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">今日計畫</p>
                  <h2>尚未安排學習項目</h2>
                </div>
                <CalendarDotsIcon aria-hidden="true" />
              </div>
              <p className="panel-copy">
                建立第一個 Subject，再把想完成的內容安排到今天。
              </p>
              <button className="secondary-button" type="button">
                建立學習科目
              </button>
            </article>

            <article className="panel panel-accent">
              <div className="panel-heading">
                <div>
                  <p className="eyebrow">工程基線</p>
                  <h2>{error ? "核心服務暫時無法使用" : "本機核心已就緒"}</h2>
                </div>
                {error ? (
                  <ShieldCheckIcon aria-hidden="true" />
                ) : (
                  <CheckCircleIcon aria-hidden="true" />
                )}
              </div>
              <dl className="foundation-list" aria-live="polite">
                <div>
                  <dt>應用程式</dt>
                  <dd>{status?.appVersion ?? "載入中…"}</dd>
                </div>
                <div>
                  <dt>資料庫版本</dt>
                  <dd>{status?.databaseSchemaVersion ?? "—"}</dd>
                </div>
                <div>
                  <dt>背景協調器</dt>
                  <dd>{status?.runtimeState === "running" ? "運作中" : "—"}</dd>
                </div>
              </dl>
              {error ? <p className="error-message">{error}</p> : null}
            </article>
          </section>
        </main>
      </div>
    </Tooltip.Provider>
  );
}
