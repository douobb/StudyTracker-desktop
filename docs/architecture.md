# StudyTracker Desktop 架構

本文件提供給準備理解或貢獻 StudyTracker Desktop 的開發者，說明目前已實作的工程基礎、程式碼邊界與品質檢查方式。產品功能仍在開發中；本文不描述內部產品規劃，也不把尚未實作的設計視為現有能力。

## 目前實作範圍

目前專案具備可啟動的 Tauri 桌面應用程式與本機資料基礎：

- Rust 啟動流程會建立應用程式資料目錄、開啟 SQLite、執行 migration、檢查資料庫完整性並載入預設設定。
- Runtime coordinator 管理背景資源的啟動與反向停止順序。
- Tauri 提供 `foundation_status` command，讓前端取得應用程式、資料庫、設定與 runtime 狀態。
- React 前端提供可調整視窗、亮暗色主題、錯誤狀態與工程基礎狀態畫面。

科目、任務、Session、活動追蹤與統計等產品功能尚未完成。

## 技術組成

| 區域     | 技術                                     | 目前責任                                 |
| -------- | ---------------------------------------- | ---------------------------------------- |
| 桌面殼層 | Tauri 2                                  | 視窗、應用程式生命週期與 Rust IPC        |
| 前端     | React、TypeScript、Vite                  | 畫面、主題、可存取性與短生命週期 UI 狀態 |
| 核心     | Rust                                     | 設定、ID、時間、錯誤與背景資源生命週期   |
| 儲存     | SQLite、migration                        | 本機設定保存、schema 升級與完整性檢查    |
| 驗證     | Vitest、Testing Library、axe、Cargo test | 前端互動、可存取性、領域與資料層測試     |

## 程式碼邊界

```text
src/                          React 前端
├── components/               畫面元件與應用程式殼層
├── hooks/                    前端狀態 hook
└── platform/                 型別化 Tauri IPC 呼叫

src-tauri/
├── migrations/              SQLite schema migration
└── src/
    ├── application/         應用服務與用例協調
    ├── domain/              不依賴 Tauri 或 SQLite 的核心型別與規則
    ├── ports/               儲存等外部能力介面
    ├── adapters/            SQLite 等介面實作
    ├── runtime/             背景資源生命週期
    ├── error.rs             內部錯誤與 IPC 錯誤轉換
    └── lib.rs               Tauri 組裝、啟動與 command 註冊
```

前端透過 `src/platform/` 呼叫已註冊的 Tauri command。Rust 的 application 與 domain 使用 port 描述外部依賴，由 adapter 提供實作；應用程式組裝集中在 `src-tauri/src/lib.rs`。

## 資料與錯誤處理

- SQLite 檔案位於 Tauri 提供的應用程式資料目錄。
- 啟動時會執行版本化 migration，並拒絕高於目前程式支援範圍的資料庫版本。
- Repository 介面將設定存取與 SQLite 實作分開，交易失敗不應留下部分寫入。
- IPC 錯誤只回傳穩定代碼、可顯示訊息與是否可重試，不直接暴露內部錯誤內容。
- 需要進入格式化輸出的敏感值可使用遮罩型別，避免原值出現在 Debug 或 Display 輸出。

## UI 與可存取性基線

目前應用程式殼層支援鍵盤操作、亮暗色主題、可見焦點、skip link、reduced motion 與語意化導覽。前端測試會檢查主要導覽、鍵盤主題切換、主題偏好保存、axe 自動化規則及主要設計 token 的 WCAG AA 色彩對比。

## 品質檢查

完整品質入口為：

```powershell
npm run check
```

此指令依序執行前端與 Rust 格式檢查、ESLint、Clippy、TypeScript 型別檢查、前端與 Rust 測試，以及前端 production build。詳細安裝、啟動與建置方式請參考專案根目錄 README。

## 擴充原則

- 業務狀態與規則應放在 Rust domain/application，不由 React 畫面自行推導。
- 新增外部能力時先定義 port，再由 adapter 實作，避免核心直接依賴平台或儲存細節。
- 資料格式變更應新增 migration，不直接改寫既有 migration。
- 新行為應在對應層級補上自動化測試，並維持 `npm run check` 可重複執行。
