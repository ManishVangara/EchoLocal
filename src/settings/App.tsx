import { useState, type ReactNode } from "react";
import { useAppState } from "./useAppState";
import { Onboarding } from "./Onboarding";
import { HomePage } from "./pages/Home";
import { AboutPage, GeneralPage, ModelPage, ShortcutPage, WritingPage } from "./pages/Settings";
import { Wordmark } from "./components/Controls";
import { GearIcon, HomeIcon, InfoIcon, KeyboardIcon, ModelIcon, SparkIcon } from "./components/Icons";

export type Page = "home" | "model" | "shortcut" | "writing" | "general" | "about";

const NAV: [Page, string, () => ReactNode][] = [
  ["home", "Home", HomeIcon],
  ["model", "Speech Model", ModelIcon],
  ["shortcut", "Shortcut", KeyboardIcon],
  ["writing", "Writing", SparkIcon],
  ["general", "General", GearIcon],
  ["about", "About", InfoIcon],
];

const ONBOARDED_KEY = "echolocal.onboarded";

function readOnboarded() {
  try {
    return localStorage.getItem(ONBOARDED_KEY) === "1";
  } catch {
    return false;
  }
}

export default function App() {
  const app = useAppState();
  const [page, setPage] = useState<Page>("home");
  const [onboarded, setOnboarded] = useState(readOnboarded);

  if (!app.snapshot) {
    return <div className="loading">Loading…</div>;
  }
  if (!onboarded) {
    return (
      <Onboarding
        app={app}
        onDone={() => {
          try {
            localStorage.setItem(ONBOARDED_KEY, "1");
          } catch {
            // Not persisted; onboarding shows again next launch.
          }
          setOnboarded(true);
        }}
      />
    );
  }

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Sections">
        <div className="sidebar-top" data-tauri-drag-region />
        <div className="brand" data-tauri-drag-region>
          <img src="/app-icon.png" alt="" width={28} height={28} />
          <Wordmark className="brand-name" />
        </div>
        {NAV.map(([id, label, Icon]) => (
          <button
            key={id}
            className={`nav-item ${page === id ? "active" : ""}`}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            <Icon />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      <main className="content">
        <div className="content-drag" data-tauri-drag-region />
        {app.error && (
          <div className="banner" role="alert">
            <span>{app.error}</span>
            <button className="plain" onClick={() => app.setError(null)} aria-label="Dismiss">
              ✕
            </button>
          </div>
        )}
        {page === "home" && <HomePage app={app} go={setPage} />}
        {page === "model" && <ModelPage app={app} />}
        {page === "shortcut" && <ShortcutPage app={app} />}
        {page === "writing" && <WritingPage app={app} />}
        {page === "general" && <GeneralPage app={app} />}
        {page === "about" && <AboutPage app={app} />}
      </main>
    </div>
  );
}
