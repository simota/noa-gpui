# 実装レポート

## 変更ファイル

既存のファイルは変更していない。追加だけ。

| パス | 内容 |
|---|---|
| `site/index.html` | LP 本体（日本語、セマンティック HTML、OGP、JSON-LD） |
| `site/styles.css` | すべてのスタイル（トークン → セクション順） |
| `site/main.js` | ヘッダーの状態、モバイルメニュー、スクロールで表示、テーマ切り替え、コピー（5 KB、依存なし） |
| `site/assets/minios-{1500,760}.webp` | Hero 画像（`noa-demo --scene=minios --snapshot` の切り抜き） |
| `site/assets/catalog-{1600,800}.webp`, `catalog-phone-760.webp` | UI カタログ（`--scene=catalog --snapshot`）。スマホ用は拡大した切り抜き |
| `site/assets/themes/*.webp` | 7 デザイン × ライト／ダーク、14 枚（`artifacts/lp/tools/themeshot.rs` で Headless 描画） |
| `site/assets/og.png`, `favicon.svg` | OGP 画像（1200×630）、ファビコン |
| `site/fonts/inter-var-latin.woff2`, `Inter-OFL.txt` | 同梱 Inter のサブセットとライセンス |
| `artifacts/lp/**` | レポート、スクリーンショット、Lighthouse の JSON、制作・検証スクリプト |

## 使用技術

静的な HTML / CSS / JavaScript だけ。ビルドは不要。npm や Cargo の依存は追加していない（`Cargo.toml` と `Cargo.lock` は変更なし）。

この判断の理由: リポジトリは Rust のワークスペースで、Web のフロントエンド基盤（Node、バンドラー）を持たない。LP のために持ち込むと保守の負担が増え、AGENTS.md の「依存の追加は人間に確認」にも当たる。

制作・検証にだけ使ったツール（リポジトリには入れていない）:

- Python Pillow（画像の切り抜きと WebP 変換）、fontTools と brotli（フォントのサブセット。brotli はスクラッチ領域に一時インストール）
- Playwright（Python、インストール済みの Chromium headless shell）
- Lighthouse 12.8.2（`npx`）
- テーマ画像の描画には、スクラッチ領域の使い捨てクレートを使った。`ui-platform` にパスで依存し、リポジトリの `target/` を共有した。ソースは `tools/themeshot.rs` に保存してある

## 実装した機能

- Hero: 見出し、定義文、主・副 CTA、実画面 2 枚、出典付きの実績 3 つ
- テーマギャラリー: 7 デザイン × ライト／ダークの切り替え。`aria-pressed` で状態を示し、画像をデコードしてから入れ替える
- コードブロック: 名前のバーとコピーボタン。クリップボードが使えないときは、コードを選択状態にする
- モバイルメニュー: `aria-expanded`、Esc で閉じてボタンにフォーカスを戻す、リンクを選ぶと閉じる
- FAQ: `details` / `summary`
- 動きを減らす設定、JavaScript 無効、`main.js` の読み込み失敗（2.5 秒後に表示へ切り替え）のどれでも、内容が見える

## 主要 CTA の遷移先

| CTA | 遷移先 |
|---|---|
| 3 ステップで動かす（Hero） | `#start` |
| 始め方（ヘッダー） | `#start` |
| GitHub でソースを見る（Hero、最終 CTA、ヘッダー） | `https://github.com/simota/noa-gpui` |
| README を読む（最終 CTA） | `https://github.com/simota/noa-gpui#readme` |
| コピー（4 か所） | クリップボード（サンプル実行、hello.rs の抜粋、Cargo.toml、ミニ OS 起動） |
| 出典・ドキュメント | GitHub 上の `docs/*.md`、`examples/hello.rs`、LICENSE、Issues |

登録・ログイン・問い合わせのフォームは、プロダクトに存在しないので作っていない。

## 起動とビルド

```sh
python3 -m http.server 8790 --directory site   # http://127.0.0.1:8790/
```

ビルドの手順はない。画像を作り直す手順は `artifacts/lp/README.md` にある。

## 既存アプリへの影響

なし。Rust のソース、`Cargo.toml`、`Cargo.lock`、`scripts/`、`.github/` は変更していない。`site/` は Cargo ワークスペースの外にあり、`scripts/verify.sh` の対象にもならない。
