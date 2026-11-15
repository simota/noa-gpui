# noa-gpui ランディングページ

noa-gpui（Rust 製の GPU UI フレームワーク）の日本語 LP。製品画像は、すべて noa-gpui 自身が描いた実際の画面。

- **場所:** `site/`（静的な HTML / CSS / JS。ビルドも依存もない）
- **最終評価:** 内部評価 85 / 100（目標 90 には未達。理由は `pdca-report.md`）。Lighthouse はデザインの見直し後、モバイル・デスクトップとも 100 / 100 / 100 / 100（ラボ測定）

## デザインの見直し（2026-11-15）

テンプレート的な要素を除き、Hero 直下に「フレーム予算の物差し」（60Hz の 1 フレーム 16.7 ms の上に、実測の 0.013 ms と 1.93 ms を同じ縮尺で描く）を置いた版に置き換えた。性能の濃色帯、課題の節、英字の小見出し、背景の格子、スクロールで現れる演出は削除した。置き換える前の 3 ファイルは `pre-deslop/` にある。置き換え後の検査は `screenshots/final/` と `lighthouse/` にある。続けて文面も書き直した（見出しを中身の名前に、重複と言い過ぎを削除）。文面を書き直す前の `index.html` は `pre-deslop-copy/` にある。

## 起動

```sh
python3 -m http.server 8790 --directory site
# http://127.0.0.1:8790/
```

ビルドは不要。どの静的ホスティングでも、`site/` をそのまま配信できる。

## 画像を作り直す

```sh
# 1. デモの実画面（GPU がなければ CPU で描く）
cargo build -p desktop --release
./target/release/noa-demo --scene=minios  --snapshot=$TMPDIR/snaps/minios.png
./target/release/noa-demo --scene=catalog --snapshot=$TMPDIR/snaps/catalog.png

# 2. テーマ画像 14 枚: tools/themeshot.rs を、noa-gpui-platform（features = ["native"]）に
#    パスで依存する使い捨てクレートの main.rs として実行する
themeshot $TMPDIR/themes

# 3. 切り抜きと WebP 変換
python3 artifacts/lp/tools/build_assets.py $TMPDIR/snaps $TMPDIR/themes .

# 4. OGP 画像: tools/og.html を site/_og.html にコピーし、1200x630 で撮影して削除する
```

## 検証

```sh
python3 artifacts/lp/tools/capture.py  http://127.0.0.1:8790/ OUT_DIR   # 6 幅の撮影とレイアウトの検査
python3 artifacts/lp/tools/interact.py http://127.0.0.1:8790/ OUT_DIR   # CTA・メニュー・テーマ・コピー・キーボード・JS の失敗
```

## 成果物

| ファイル | 内容 |
|---|---|
| `product-analysis.md` | プロダクト分析、ペルソナ、確認できた事実と推測の区別 |
| `creative-strategy.md` | コンセプト、コピー、コンバージョン設計、構成 |
| `design-spec.md` | カラー、タイポグラフィ、余白、レスポンシブ、モーション、コンポーネント |
| `implementation-report.md` | 変更ファイル、CTA の遷移先、既存アプリへの影響 |
| `pdca-report.md` | 4 サイクルの評価、改善、検証、独立レビュー |
| `qa-report.md` | テスト結果、Lighthouse、未検証の項目 |
| `screenshots/` | `initial/`、`iterations/cycle-1〜4/`、`final/` |
| `lighthouse/` | Lighthouse の JSON（モバイル、デスクトップ） |
| `tools/` | 画像の制作と検証のスクリプト |

## 既知の制約

- **公開 URL が未定。** `og:image` が相対パスで、`og:url` と canonical もない。#TODO(agent): 公開先が決まったら、`site/index.html` の `og:image` を絶対 URL にし、`og:url` と `<link rel="canonical">` を追加する
- **ブラウザで試せるデモがない。** `apps/web` のビルドを公開すれば、Hero の主 CTA にできる（いちばん効果の大きい改善候補）
- **GitHub リポジトリの公開状態と、リンクの到達性は未確認。**
- **「795 件のテスト」は 2026 年 11 月時点の数**（`grep '#[test]'` による）。増減したら FAQ を更新する
- 確認したブラウザは Chromium だけ（Safari、Firefox、実機のスマホは未確認）
- Hero 画像の UI の文字は、スマホでは小さい（デスクトップアプリの画面を縮めているため）
