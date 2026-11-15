# デザイン仕様

実装は `site/styles.css`（トークンを `:root` に定義）。

## カラー

| トークン | 値 | 用途 |
|---|---|---|
| `--paper` | `#f4f5f8` | ページ背景（Big Sur ライトの背景と同じ） |
| `--paper-2` | `#eceef4` | インラインコード、ホバー |
| `--surface` | `#ffffff` | 白いセクション、ボタン |
| `--ink` | `#12141b` | 見出し、濃色セクション、コード |
| `--text` / `--muted` | `#1e2230` / `#535a6b` | 本文 / 補足（紙色の上で 6.3:1、白の上で 6.9:1） |
| `--line` / `--line-strong` | `#dbdfe8` / `#c3c9d6` | 罫線 |
| `--amber` | `#e8a33d` | 主 CTA、強調数値、マーカー（プロダクトの `NOA_ACCENT`） |
| `--amber-ink` | `#7d5208` | 紙色の上のアンバー系文字（キッカー） |
| `--night-text` / `--night-muted` | `#e8eaf0` / `#a6adbd` | 濃色セクションの文字 |

アンバーは面としてだけ使い、文字色にはしない（白背景で 2.16:1 しかないため）。主ボタンは アンバー地にインク文字。

## タイポグラフィ

- 欧文: Inter（リポジトリ同梱の `crates/ui-text/assets/fonts/Inter.ttf` を Latin 範囲にサブセット、`opsz` を固定した可変ウェイトの woff2、36 KB、`font-display: swap`、`unicode-range` 付き）
- 和文: システムフォント（ヒラギノ角ゴ → Noto Sans JP → 游ゴシック UI → メイリオ）。Web フォントの和文は読み込まない（数 MB になるため）
- 等幅: `ui-monospace`, SF Mono, Menlo, Consolas
- 見出し: 700、字間 -0.03〜-0.035em、`palt` で和文の字詰め。改行位置は `<span class="nb">`（inline-block）で句の途中で折れないようにした
- 本文: 16px / 行間 1.8〜1.9
- サイズ: Hero `clamp(2.4rem, …, 4.5rem)`（1024px 以上は列幅に合わせて別の clamp）、節見出し `clamp(1.75rem, …, 2.75rem)`

## スペーシングとレイアウト

- コンテンツ幅 1200px、左右の余白 `clamp(16px, 4vw, 40px)`
- セクションの上下 `clamp(80px, 10vw, 144px)`
- 2 カラム節は 5:6 の非対称（Hero、デザイン、仕組み）、4:7（始め方、FAQ）
- 角丸 14px（額縁）、12px（コード・層）、999px（ボタン・チップ）

## レスポンシブ方針

| 幅 | 主な変化 |
|---|---|
| < 600px | Hero は 文 → CTA（全幅・縦積み）→ 実画面 → 実績値の順。実画面は右端まで裁ち落とし。カタログはモバイル専用の拡大切り抜き（`catalog-phone-760.webp`）。コードは全幅、シェルと Cargo.toml は折り返し |
| < 880px | ナビゲーションを開閉メニューに（44px のボタン、Esc で閉じる、リンク選択で閉じる） |
| < 900px | 性能の数値を 1 列、始め方のステップを 1 列 |
| ≥ 1024px | Hero が 2 カラム。実画面が縦 2 段分にまたがる |

## アニメーション

- 初回表示: Hero の要素が 14px 下からフェードイン（0.8s、60ms ずつずらす）。テーマパネルは 0.45s 遅れて入る
- スクロール: `.reveal` が 20px 下からフェードイン（IntersectionObserver、一度だけ）
- テーマ切り替え: 画像をデコードしてから 0.25s でクロスフェード
- イージング `cubic-bezier(.2, .7, .2, 1)`
- `prefers-reduced-motion: reduce` ではすべて即時表示。JavaScript 無効時も全要素が表示される（`.js` クラスがある時だけ初期状態を隠す）
- transform と opacity だけを動かし、レイアウトシフトを起こさない

## コンポーネント

| 名前 | クラス | 備考 |
|---|---|---|
| ボタン | `.btn-primary` / `.btn-ghost` / `.btn-ghost-dark` | 高さ 48px 以上 |
| 額縁 | `.frame` | 実画面用。影 3 段 |
| コードブロック | `.code-block` + `.code-bar` + `.code` | 名前とコピーボタンをバーに分離（サイクル 2 でコードとボタンの重なりを解消） |
| テーマ切り替え | `[data-design]` / `[data-appearance]` | `aria-pressed` で状態を示すボタン群 |
| 層構成図 | `.stack > .layer` | `ol` で順序を表す |
| FAQ | `details` / `summary` | JavaScript 不要 |
