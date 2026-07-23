import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("./platform/foundation", () => ({
  getFoundationStatus: vi.fn().mockResolvedValue({
    appVersion: "0.1.0",
    databaseSchemaVersion: 1,
    settingsSchemaVersion: 1,
    runtimeState: "running",
  }),
}));

describe("App", () => {
  beforeEach(() => localStorage.clear());
  afterEach(cleanup);

  it("呈現主要導覽與本機核心狀態", async () => {
    render(<App />);
    expect(
      screen.getByRole("navigation", { name: "功能" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "今天想完成什麼？" }),
    ).toBeInTheDocument();
    expect(await screen.findByText("運作中")).toBeInTheDocument();
  });

  it("可用鍵盤切換主題並保存偏好", async () => {
    const user = userEvent.setup();
    render(<App />);
    const toggle = screen.getByRole("button", { name: "切換為暗色模式" });
    toggle.focus();
    await user.keyboard("{Enter}");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("studytracker.theme")).toBe("dark");
  });

  it("應用殼層沒有自動化可存取性違規", async () => {
    render(<App />);
    await screen.findByText("運作中");
    const results = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false } },
    });
    expect(results.violations).toEqual([]);
  });
});
