import { useEffect, useState } from "react";
import { AppShell } from "./components/app-shell";
import { useTheme } from "./hooks/use-theme";
import {
  getFoundationStatus,
  type FoundationStatus,
} from "./platform/foundation";

export default function App() {
  const { theme, toggleTheme } = useTheme();
  const [status, setStatus] = useState<FoundationStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    getFoundationStatus()
      .then((result) => {
        if (active) setStatus(result);
      })
      .catch(() => {
        if (active) setError("無法讀取本機核心狀態，請重新啟動應用程式。");
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <AppShell
      status={status}
      error={error}
      theme={theme}
      onToggleTheme={toggleTheme}
    />
  );
}
