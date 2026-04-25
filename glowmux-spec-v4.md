# glowmux 仕様書 v4.0

## アプリ概要

**glowmux** — Agent Multiplexer with Glow

エージェント向けターミナルマルチプレクサ。dmux × ccmux × cc-glow の統合に加え、
世界で唯一「セッション内容をリアルタイムでAIタイトル化」する機能を持つ。

| 項目 | 内容 |
|------|------|
| アプリ名 | glowmux |
| ベース | ccmux（MIT License）をフォーク |
| リポジトリ | AniP-gt/glowmux |
| ライセンス | MIT（ccmuxの著作権表示を保持 + 独自著作権を追加） |
| 言語 | Rust |
| 対応エージェント | Claude Code / Codex / OpenCode / Gemini CLI など汎用 |

---

## DNAマップ

```
ccmux       → PTY管理・TUI・ファイルツリー・diff・シンタックスハイライト
dmux        → git worktree自動管理・並列エージェント・worktree名生成
cc-glow     → Claude Code hooks連携・状態ドット・背景色変化
glowmux独自 → AIタイトル自動生成（Ollama / Gemini / Claude Headless）
```

---

## 機能一覧

### ccmuxから継承（そのまま保持）

- PTY管理・vt100エミュレーション
- ペイン分割（縦・横）・マウスリサイズ
- タブワークスペース
- ファイルツリーサイドバー（アイコン付き・展開/折畳）
- シンタックスハイライト付きファイルプレビュー
- diffプレビュー
- Claude Code検知（枠色変化）
- cd追跡（ディレクトリ移動でファイルツリー自動更新）
- スクロールバック（デフォルト10,000行・設定可能）
- マウスサポート全般

### レイアウト

**起動時自動レイアウト**

- 起動時にターミナルサイズを取得し `auto_responsive = true` の場合はサイズに応じて自動決定
- `startup.panes` で指定したペイン数を生成（サイズ優先・ペイン数を保証）
- 前回セッションがある場合はセッション復元を優先し startup は無視

```
起動フロー
glowmux起動
    ↓
session.auto_restore = true かつ前回セッションあり？
    Yes → セッション復元（startupは無視）
    No  → ターミナルサイズ取得
            ↓
          auto_responsive に従いレイアウト決定
            ↓
          startup.panes の数だけペイン生成
```

**レスポンシブ自動レイアウト**

- `< breakpoint_stack 列` → 縦積み
- `breakpoint_stack 〜 breakpoint_split2 列` → 左右2分割
- `> breakpoint_split2 列` → 上下左右4分割（グリッド）
- ウィンドウリサイズ時もリアルタイムで追従

**ペインズーム**

- `Ctrl+Z` でフォーカスペインを全画面表示
- 再度 `Ctrl+Z` で元のレイアウトに戻る

**レイアウト切替**

- `Ctrl+Space` でレイアウトを順番に切替（tmuxのPrefix+Spaceと同等）
  - 縦積み → 左右分割 → グリッド → メイン+サブ → 縦積み → ...
  - ペイン数に応じて適用可能なレイアウトのみをサイクル
- `Ctrl+L` でレイアウト選択UIを表示

```
レイアウト選択UI（Ctrl+L）
┌─────────────────────────────────────┐
│  [1] 縦積み      [2] 左右分割        │
│  ┌──┐            ┌──┬──┐            │
│  │  │            │  │  │            │
│  ├──┤            └──┴──┘            │
│  │  │                               │
│  └──┘                               │
│                                     │
│  [3] 上下分割    [4] グリッド        │
│  ┌──┐            ┌──┬──┐            │
│  │  │            │  │  │            │
│  ├──┤            ├──┼──┤            │
│  │  │            │  │  │            │
│  └──┘            └──┴──┘            │
│                                     │
│  [5] メイン+サブ [6] 大1+小3        │
│  ┌────┬──┐       ┌────┬──┐          │
│  │    │  │       │    ├──┤          │
│  │    ├──┤       │    ├──┤          │
│  │    │  │       │    │  │          │
│  └────┴──┘       └────┴──┘          │
└─────────────────────────────────────┘
数字キーで即選択 / Ctrl+Spaceで順送り
```

### 状態通知（cc-glowのDNA）

- Claude Code hooks受信（Unix socket）
- ペインヘッダーの状態ドット
  - 🔵 実行中（running）
  - 🟢 完了（done）
  - 🟡 確認待ち（waiting）
- ペイン背景色変化（完了=暗緑、確認待ち=暗黄）
- `respect_terminal_bg = true` でターミナル側の透過・背景を優先
  - 背景色変化を無効化し、ドット・枠色のみで状態を表現
- フォーカス時に自動ディスミス（ドット消去）
- グローバルステータスバー（ボトム）
  - `実行中:2 / 完了:1 / 待機:1 | worktrees:3 | AI:online | 14:32`

### AIタイトル自動生成（独自の核心）

- PTY出力をリングバッファでキャプチャ
- ANSIエスケープシーケンスのストリップ処理
- 会話ターン検知（プロンプト復帰を検知）でトリガー
- 非同期リクエスト（tokio spawn）・タイムアウト設定
- タイトルをペインヘッダーにリアルタイム更新
- `Ctrl+A` でON/OFF即切替
  - ON  → `● [1] N+1クエリ調査  fix/n-plus-one  🟢`
  - OFF → `● [1] pane-1         fix/n-plus-one  🟢`
- バックエンドを用途別に個別設定可能（後述）

### worktree管理（dmuxのDNA）

- gwq優先 / 内蔵git worktreeフォールバック
  - `gwq` コマンドが存在する場合は自動的に優先使用
  - 存在しない場合は `git worktree` コマンドで内蔵実装
- 新ペイン作成時にworktree自動生成（設定でon/off可能）
- worktree名はAI生成 or 手動入力を選択可能
- ペイン作成時のインタラクティブUI（後述）
- マージ後の自動cleanup通知（「クローズしますか？」ダイアログ）
- ペインヘッダーにブランチ名表示

### ペイン作成インタラクティブUI

新規ペイン作成時（Ctrl+N）に以下のダイアログを表示：

```
新規ペイン作成
┌──────────────────────────────────┐
│ ブランチ名: [fix/n-plus-one    ] │  ← AI生成 or 手動入力
│ worktree:   [✅ 作成する       ] │  ← トグルで都度切替可
│ エージェント: [claude          ] │
│                                  │
│ [AI生成]           [OK] [Cancel] │
└──────────────────────────────────┘
```

- `worktree` トグルは config.toml の `auto_create` がデフォルト値
- `AI生成` ボタンでブランチ名をAIが自動生成（dmux相当）

### ペイン削除

- `Ctrl+W` でフォーカスペインを削除（キー変更可能）
- 削除時の挙動は設定で制御

```toml
[pane]
close_confirm  = true    # true=確認ダイアログ / false=即削除
close_worktree = "ask"   # ask | auto | never（worktreeも削除するか）
```

### ペイン移動

- デフォルトは `Alt+hjkl`（自由に変更可能）
- よくある設定例をコメントで提供

### ペイン間コンテキスト共有

- `Ctrl+Y` でフォーカスペインの出力（直近N行）をクリップボードへコピー
- ペイン間貼り付けショートカット

### セッション永続化

- `~/.config/glowmux/session.json` にペイン構成・タイトル・worktreeを保存
- glowmux再起動時に前回構成を自動復元（startupより優先）
- 復元時にClaude Codeを自動再起動するかを選択可能（デフォルト: false）

### フィーチャートグルUI（`?` キー）

いつでも `?` キーで機能のON/OFFを切替可能。変更はリアルタイム反映・config.tomlに即時書き込み・再起動不要。

```
? キー押下
┌─────────────────────────────────────┐
│ glowmux features                    │
├─────────────────────────────────────┤
│ [✅] status_dot        状態ドット   │
│ [✅] status_bg_color   背景色変化   │
│ [✅] status_bar        ステータスバー│
│ [✅] worktree          worktree統合 │
│ [✅] worktree_ai_name  worktree名AI │
│ [✅] file_tree         ファイルツリー│
│ [✅] file_preview      ファイル表示 │
│ [✅] diff_preview      diff表示     │
│ [✅] cd_tracking       cd追跡       │
│ [✅] ai_title          AIタイトル   │
│ [✅] responsive_layout レスポンシブ │
│ [✅] session_persist   セッション   │
│ [✅] context_copy      コンテキスト │
│ [✅] layout_picker     レイアウトUI │
│ [✅] startup_panes     自動分割     │
├─────────────────────────────────────┤
│ j/k移動  Space切替  q閉じる         │
└─────────────────────────────────────┘
```

### TUI内設定パネル

- `Ctrl+,` で設定パネルを開く
- 主要設定をリアルタイム変更・config.tomlに即時反映
- 再起動不要

---

## ペインヘッダーイメージ

```
● [1] N+1クエリ調査  fix/n-plus-one  🟢
● [2] webhook実装    fix/webhook      🔵
  [3] 待機中                          ⚪
```

---

## グローバルステータスバー（ボトム）

```
[glowmux] 実行中:2  完了:1  待機:1 | worktrees:3 | AI:online | 2026-04-25 14:32
```

---

## 技術スタック

| 要素 | 採用 | 備考 |
|------|------|------|
| 言語 | Rust | ccmux・rtdの延長 |
| TUI | ratatui + crossterm | ccmuxから継承 |
| PTY | portable-pty | ccmuxから継承 |
| 端末エミュレーション | vt100 | ccmuxから継承 |
| 非同期 | tokio | AIタイトル生成・hooks受信 |
| HTTP | reqwest | Ollama / Gemini API |
| シンタックスハイライト | syntect | ccmuxから継承 |
| クリップボード | arboard | ccmuxから継承 |
| hooks受信 | Unix socket | cc-glowのDNA |
| 設定 | toml クレート + serde | ユーザー設定 |
| 永続化 | serde_json | セッション保存 |

---

## 設定ファイル

### ファイル構成

```
~/.config/glowmux/
├── config.toml      # ユーザー設定（人間が編集）
└── session.json     # セッション永続化（アプリが読み書き）
```

### config.toml（全設定項目）

```toml
# -----------------------------------------------
# 機能ON/OFF（? キーからもリアルタイム切替可能）
# -----------------------------------------------

[features]
# cc-glow系
status_dot         = true   # 状態ドット（🔵🟢🟡）
status_bg_color    = true   # ペイン背景色変化
status_bar         = true   # ボトムステータスバー

# dmux系
worktree           = true   # git worktree統合
worktree_ai_name   = true   # worktree名AI生成

# ccmux系
file_tree          = true   # ファイルツリーサイドバー
file_preview       = true   # ファイルプレビュー
diff_preview       = true   # diffプレビュー
cd_tracking        = true   # cd追跡（ファイルツリー自動更新）

# glowmux独自
ai_title           = true   # AIタイトル自動生成
responsive_layout  = true   # レスポンシブ自動レイアウト
session_persist    = true   # セッション永続化
context_copy       = true   # ペイン間コンテキスト共有
layout_picker      = true   # レイアウト選択UI
startup_panes      = true   # 起動時自動ペイン分割

# -----------------------------------------------
# 端末設定
# -----------------------------------------------

[terminal]
scrollback_lines = 10000        # スクロールバック行数
theme = "dark"                  # dark | light | custom

# -----------------------------------------------
# レイアウト設定
# -----------------------------------------------

[layout]
auto_responsive = true          # レスポンシブ自動レイアウト
breakpoint_stack = 120          # この列数未満 → 縦積み
breakpoint_split2 = 200         # この列数未満 → 左右2分割
                                # breakpoint_split2以上 → グリッド

# -----------------------------------------------
# 起動時自動ペイン分割
# session.auto_restore = true かつ前回セッションがある場合は
# セッション復元が優先されこの設定は無視される
# -----------------------------------------------

[startup]
enabled = true                  # 起動時自動レイアウトを有効化

[[startup.pane]]
command = "claude"              # 起動するコマンド（空ならシェルのみ）
worktree = true                 # worktreeを自動生成するか
branch = ""                     # 空ならAI生成 or インタラクティブUI

[[startup.pane]]
command = "claude"
worktree = true
branch = ""

[[startup.pane]]
command = ""                    # コマンドなし（シェルのみ）
worktree = false

# -----------------------------------------------
# ペイン設定
# -----------------------------------------------

[pane]
close_confirm  = true           # true=確認ダイアログ / false=即削除
close_worktree = "ask"          # ask | auto | never（worktreeも削除するか）

# -----------------------------------------------
# AIバックエンド設定
# 用途ごとに異なるバックエンドを指定可能
# backend: ollama | gemini | claude-headless
# -----------------------------------------------

[ai.title]
# セッション内容のタイトル自動生成
backend = "claude-headless"     # 高速・高精度なhaikuを推奨
model = "claude-haiku-4-5"
timeout_sec = 5
update_interval_sec = 30        # タイトル更新間隔（秒）
max_chars = 12                  # タイトル最大文字数（日本語想定）

[ai.worktree_name]
# worktreeブランチ名のAI生成（dmux相当）
backend = "claude-headless"
model = "claude-haiku-4-5"
timeout_sec = 10

[ai.ollama]
url = "http://localhost:11434"
model = "qwen2.5-coder:14b"

[ai.gemini]
api_key = ""

[ai.claude_headless]
# claude codeがインストールされていれば追加設定不要
model = "claude-haiku-4-5"      # haiku推奨（高速・低コスト）
timeout_sec = 10

# -----------------------------------------------
# バックエンド選択ガイド
# claude-headless → 高速・高精度・Claude Code導入済みなら即使える
#                   ただしAPI経由のため業務コードは注意
# ollama          → 完全ローカル・業務コードも安全・要モデルDL
# gemini          → API経由・要APIキー
# -----------------------------------------------

# -----------------------------------------------
# 状態通知設定
# -----------------------------------------------

[status]
color_running = "#66D9EF"       # 実行中ドット色
color_done    = "#A6E22E"       # 完了ドット色
color_waiting = "#E6DB74"       # 確認待ちドット色
bg_done       = "#0d2b0d"       # 完了時ペイン背景色
bg_waiting    = "#2b1a00"       # 確認待ちペイン背景色
bg_reset      = ""              # デフォルト背景（空=端末デフォルト）
indicator     = "●"             # 状態ドット文字
respect_terminal_bg = false     # trueでターミナル側の透過・背景を優先
                                # trueの場合 bg_done/bg_waiting は無視され
                                # ドット・枠色のみで状態を表現
override_bg_done    = true      # 完了時の背景色上書きをするか
override_bg_waiting = true      # 確認待ちの背景色上書きをするか

# -----------------------------------------------
# worktree設定
# -----------------------------------------------

[worktree]
prefer_gwq = true               # gwqを優先使用（falseで常に内蔵）
gwq_basedir = "~/ghq"           # gwqのbasedirと合わせる
auto_create = true              # ペイン作成時にworktree自動生成
                                # ペイン作成UIでも都度切替可能
cleanup_on_close = "ask"        # ask | auto | never

# -----------------------------------------------
# セッション設定
# -----------------------------------------------

[session]
auto_save = true                # 終了時に自動保存
auto_restore = true             # 起動時に自動復元（startupより優先）
save_path = "~/.config/glowmux/session.json"
restore_claude = false          # 復元時にClaude Codeを自動起動するか

# -----------------------------------------------
# キーバインド設定（全て自由に変更可能）
# -----------------------------------------------

[keybindings]
# ペイン操作
zoom             = "ctrl+z"     # ペインズーム
new_pane         = "ctrl+n"     # 新規ペイン（インタラクティブUI）
close_pane       = "ctrl+w"     # ペイン削除
split_vertical   = "ctrl+d"     # 縦分割
split_horizontal = "ctrl+e"     # 横分割

# ペイン移動（デフォルト: alt+hjkl）
pane_left        = "alt+h"
pane_down        = "alt+j"
pane_up          = "alt+k"
pane_right       = "alt+l"
# 矢印キー派はこちら
# pane_left      = "ctrl+left"
# pane_down      = "ctrl+down"
# pane_up        = "ctrl+up"
# pane_right     = "ctrl+right"
# vimライク派はこちら（layout_pickerの変更も必要）
# pane_left      = "ctrl+h"
# pane_down      = "ctrl+j"
# pane_up        = "ctrl+k"
# pane_right     = "ctrl+l"
# layout_picker  = "ctrl+p"

# レイアウト
layout_cycle     = "ctrl+space" # レイアウト順送り（tmux Prefix+Space相当）
layout_picker    = "ctrl+l"     # レイアウト選択UI

# AI
ai_title_toggle  = "ctrl+a"     # AIタイトルON/OFF

# その他
context_copy     = "ctrl+y"     # ペイン出力をクリップボードへ
new_tab          = "ctrl+t"     # 新規タブ
toggle_filetree  = "ctrl+f"     # ファイルツリー表示切替
settings         = "ctrl+,"     # 設定パネルを開く
feature_toggle   = "?"          # フィーチャートグルUI
quit             = "ctrl+q"     # 終了
```

---

## ライセンス表記

```
MIT License

Copyright (c) 2026 Shin-sibainu   ← 保持必須
Copyright (c) 2026 AniP-gt        ← 追加

Built upon ccmux by Shin-sibainu (MIT License)
https://github.com/Shin-sibainu/ccmux
```

---

## 開発フェーズ

| Phase | 内容 | 優先度 |
|-------|------|--------|
| 1 | ccmuxフォーク・リネーム・ペインズーム・レスポンシブ・レイアウト切替UI | 最優先 |
| 2 | 起動時自動ペイン分割（startup設定）・ペイン移動キー | 最優先 |
| 3 | フィーチャートグルUI（?キー）・features設定 | 高 |
| 4 | cc-glow統合・状態ドット・背景色・ステータスバー・透過対応 | 高 |
| 5 | AIタイトル生成（Claude Headless Haiku / Ollama 非同期）・Ctrl+Aトグル | 高 |
| 6 | gwq/worktree統合・ペイン作成UI・cleanup通知 | 中 |
| 7 | worktree名AI生成（Claude Headless Haiku） | 中 |
| 8 | ペイン間コンテキスト共有（Ctrl+Y） | 中 |
| 9 | セッション永続化（session.json） | 中 |
| 10 | TUI内設定パネル（Ctrl+,） | 低 |

---

## 競合比較

| 機能 | glowmux | dmux | tamux | ccmux |
|------|:-------:|:----:|:-----:|:-----:|
| AIタイトル自動生成 | ✅ | ❌ | ❌ | ❌ |
| Claude Headlessバックエンド | ✅ | ❌ | ❌ | ❌ |
| git worktree統合 | ✅ | ✅ | ❌ | ❌ |
| worktree名AI生成 | ✅ | ✅ | ❌ | ❌ |
| 状態ドット通知 | ✅ | △ | ✅ | △ |
| ペイン背景色変化 | ✅ | ❌ | ❌ | ❌ |
| 透過端末対応 | ✅ | ❌ | ❌ | ❌ |
| ファイルツリー | ✅ | ❌ | ❌ | ✅ |
| diffプレビュー | ✅ | ❌ | ❌ | ✅ |
| セッション永続化 | ✅ | ❌ | ✅ | ❌ |
| ペインズーム | ✅ | ❌ | ❌ | ❌ |
| レイアウト切替UI | ✅ | ❌ | ❌ | ❌ |
| レスポンシブレイアウト | ✅ | ❌ | ❌ | ❌ |
| 起動時自動ペイン分割 | ✅ | ❌ | ❌ | ❌ |
| ペイン移動ショートカット | ✅ | ❌ | ❌ | ❌ |
| フィーチャートグルUI | ✅ | ❌ | ❌ | ❌ |
| AIタイトルON/OFFトグル | ✅ | ❌ | ❌ | ❌ |
| ローカルLLM対応（Ollama） | ✅ | ❌ | ❌ | ❌ |
| 設定ファイル（TOML） | ✅ | ❌ | ✅ | ❌ |
| ペイン作成インタラクティブUI | ✅ | △ | ❌ | ❌ |
| 全機能ON/OFF設定 | ✅ | ❌ | ❌ | ❌ |
