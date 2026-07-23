# StudyTracker Desktop

[English](README.en.md)

StudyTracker Desktop 是一款以個人使用為核心的桌面學習規劃、專注計時與活動回顧應用程式。產品採本地優先與隱私優先設計，目標是在沒有帳號或網路連線時，仍能管理學習內容並保有自己的紀錄。

> 目前已完成可啟動的工程骨架與本機資料基礎；科目、Session、活動追蹤等產品功能仍在開發中，尚無正式發布版本。

## 核心方向

- 管理科目、任務與每日學習計畫。
- 提供正計時、倒數計時與番茄鐘。
- 記錄桌面應用程式活動，並讓使用者控制記錄範圍。
- 透過可調整的黑名單、白名單與提醒機制協助維持專注。
- 提供本地統計、資料匯出、備份與還原。
- 核心功能可離線使用，不以帳號或雲端服務為必要條件。

以上產品功能目前皆屬規劃內容，不代表已完成實作。

## 目前已建立

- Tauri 2、Rust、React、TypeScript 與 Vite 應用程式骨架。
- SQLite migration、Repository 與交易邊界。
- ID、時鐘、時區、設定、安全日誌及背景協調器基礎。
- 支援亮色／暗色與鍵盤操作的可存取應用程式殼層。
- 前端與 Rust 的格式、lint、型別、測試及建置檢查入口。

## 開發需求

- Node.js 24 或以上
- npm 11 或以上
- Rust 1.85 或以上
- Windows 上建置 Tauri 所需的系統相依套件

## 開始開發

```powershell
npm install
npm run tauri:dev
```

執行完整品質檢查：

```powershell
npm run check
```

建立桌面應用程式：

```powershell
npm run tauri:build
```

目前建置不產生安裝程式；正式封裝與發布仍待後續完成。

## 文件

- [目前架構與開發邊界](docs/architecture.md)

## StudyTracker 專案

Desktop 是 StudyTracker 的桌面客戶端。主專案儲存庫尚未建立，建立後會補上正式連結。

未來預計依不同作業系統與裝置能力，逐步開發其他桌面及行動裝置版本。

## 授權

本專案採 [Apache License 2.0](LICENSE) 授權。
