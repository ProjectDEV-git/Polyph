# The website (apps/web): marketing pages, pricing, download, security,
# legal drafts and the account page.

## Shared
site-title = Polyph：チームのコーディングエージェントを 1 つのルームに
site-description = 各自が自分のマシンで自分のコーディングエージェントを動かします。Polyph はそれらをエンドツーエンド暗号化された 1 つのルームにまとめ、互いの作業を見せ、編集前にファイルを確保させ、ぶつからずにマージさせます。
site-nav-label = サイト
site-nav-home = Polyph ホーム
site-nav-pricing = 料金
site-nav-download = ダウンロード
site-nav-security = セキュリティ
site-nav-account = アカウント
site-nav-privacy = プライバシー（草案）
site-nav-terms = 利用規約（草案）
site-nav-source = ソースコード
site-lang-label = 言語
site-theme-dark = ダーク
site-theme-light = ライト
site-theme-label = テーマ
site-skip = 本文へ移動

## Home: hero
site-hero-title = チームのコーディングエージェントを、1 つのルームに。
site-hero-sub = 各自が自分のマシンで自分のエージェントを動かします。Polyph はエージェント同士に互いの作業を見せ、編集前にファイルを確保させ、ぶつからずにマージさせます。
site-hero-facts-label = 概要
site-fact-platforms = Linux、macOS、Windows（Bazzite と Fedora Atomic を含む）
site-fact-e2e = エンドツーエンド暗号化：リレーは暗号文を転送するだけ
site-fact-selfhost = リレーは小さな単一バイナリで、自分でホスト可能
site-fact-agents = どのエージェントでも、自分のキーで
site-fact-langs = English、ไทย、日本語、简体中文
site-rail-agents = Claude Code、Codex、OpenCode、ACP エージェント、ローカルモデル
site-rail-code = 8 文字のコードで参加
site-rail-lan = インターネットなしの LAN でも動作
site-rail-open = エンジンとアプリは Apache-2.0
site-rail-action = Polyph をダウンロード

## Interface fragments (sample room)
site-ui-room = payments
site-ui-pill-running = 実行中 { $count }
site-ui-pill-waiting = 承認待ち { $count }
site-ui-agent-of = { $name } のエージェント
site-ui-line-1 = 編集前に src/pay/refund.ts を確保しました。
site-ui-line-2 = refund.ts は { $name } のエージェントが確保しています。代わりに webhook ハンドラーを進めます。
site-ui-line-3 = テストが通りました。レビューに回します。
site-ui-line-4 = タイムアウト時に返金を再試行しますか？カードでは「はい」です。
site-ui-line-5 = 両方をマージしました：タイムアウト時の再試行と冪等キー。
site-ui-approve-title = { $name } のエージェントがコマンドの実行を求めています
site-ui-approve-cmd = コマンド
site-ui-approve-files = ファイル
site-ui-approve-hosts = ネットワーク
site-ui-approve-none = なし
site-ui-approve-once = 今回だけ承認
site-ui-approve-task = このタスクで承認
site-ui-deny = メモを付けて拒否
site-ui-status-running = 実行中
site-ui-status-waiting = 承認待ち
site-ui-status-blocked = ブロック中
site-ui-status-done = 完了
site-ui-claimed-by = { $name } が確保
site-ui-knock = { $name } が参加を求めています
site-ui-approve = 承認
site-ui-merged = main にマージ済み
site-ui-kept-both = 両方の意図を保持
site-ui-col-todo = 未着手
site-ui-col-doing = 作業中
site-ui-col-review = レビュー
site-ui-card-1 = タイムアウト時に返金を再試行
site-ui-card-2 = 返金の冪等キー
site-ui-card-3 = webhook の署名チェック

## Home: live room
site-show-title = ルームを作る
site-show-sub = チームを選んで、誰が何を持っているかを見てください。各人に色が付き、その人のエージェントには同じ色の明るい段階と暗い段階が付きます。Polyph と同じ方法で割り当てています。
site-show-controls = ルームの設定
site-show-people = 人数
site-show-agents = 1 人あたりのエージェント
site-show-claims = 確保
site-show-approvals = コマンド前に確認
site-show-theme = ライトテーマ
site-show-mode = 共有作業コピー
site-show-fewer = 減らす
site-show-more = 増やす
site-show-roster = このルームのメンバー
site-show-mode-distributed = タスクごとに専用の worktree
site-show-mode-hosted = ホスト上の 1 つの共有作業コピー
site-show-claims-off = 確保がオフです。エージェントが同じファイルを編集する可能性があります。
site-show-next = 次にリポジトリで polyph host を実行し、表示されたコードを共有します。
site-show-link = 仕組み

## Home: how it works
site-how-title = コードからマージまで
site-how-host = ホスト
site-how-host-text = リポジトリでルームを作ります。Polyph が参加コードを表示します。
site-how-join = 参加
site-how-join-text = チームメイトがコードを入力します。何かが見える前に、1 人ずつ承認します。
site-how-plan = 計画と確保
site-how-plan-text = プランナーが目標をカードに分けます。エージェントは編集前にファイルを確保するので、同じファイルを 2 つが書くことはありません。
site-how-merge = レビューとマージ
site-how-merge-text = レビュー用エージェントが各タスクを確認します。コンフリクトは、両方のタスクの意図を残す形でマージされます。

## Home: close-ups
site-parts-title = ルームにあるもの
site-part-claims = 確保
site-part-claims-text = ファイル、フォルダー、または 1 つの関数を、編集前に確保。
site-part-approvals = 承認
site-part-approvals-text = 実行前に、正確なコマンド、ファイル、ホストを表示。
site-part-code = 参加コード
site-part-code-text = 8 文字、1 回限り、既定で承認が必要。
site-part-relay = 小さなリレー
site-part-relay-text = 5 人のルーム 50 個での負荷試験時のピークメモリ。
site-part-status = どこでも状態を確認
site-part-status-text = デスクトップ、スマートフォン、VS Code、ターミナル、最前面のピル。
site-part-board = ボード
site-part-board-text = カンバン、スレッド、再生できるアクティビティ、ホワイトボード。
site-part-langs = 4 つの言語
site-part-langs-text = すべての画面とコマンドが英語、タイ語、日本語、中国語に対応。

## Home: surfaces
site-surfaces-title = すべての画面
site-surfaces-count = { $count } つの画面、その下に 1 つのエンジン。
site-surface-open = 詳しく見る
site-surface-cli = コマンドライン
site-surface-cli-text = すべての操作をコマンドで：ホスト、参加、エージェント起動、承認、計画、マージ。スクリプトは JSON を読めます。
site-surface-tui = ターミナル UI
site-surface-tui-text = 引数なしで polyph を実行すると、チャット、ボード、エージェント、承認が表示されます。SSH 越しでも動作します。
site-surface-desktop = デスクトップアプリ
site-surface-desktop-text = エディターの横にルーム全体を。トレイアイコンとネイティブ通知付き。
site-surface-browser = ブラウザー
site-surface-browser-text = 同じアプリをブラウザーのタブで。自分のマシンから 1 回限りのリンクで開きます。
site-surface-phone = スマートフォン
site-surface-phone-text = スマートフォンからルームを追い、エージェントを承認。ブラウザーからインストールします。
site-surface-vscode = VS Code
site-surface-vscode-text = 開いているファイルに確保を表示し、承認を通知として受け取ります。
site-surface-pill = ステータスピル
site-surface-pill-text = 最前面に置ける小さなウィンドウ。最初の承認にその場で答えられます。
site-surface-everywhere = Linux、macOS、Windows
site-surface-phones = iOS と Android のブラウザー
site-surface-editor = すべてのデスクトップの VS Code

## Marquee
site-marquee = 独立した声、1 つのルーム

## Pricing
site-pricing-title = プラン
site-pricing-h1 = ホスト型リレーのプラン
site-pricing-sub = Polyph はアカウントなしで使えます。プランを使うと、ホスト型リレーでのルーム保存期間が延び、請求をまとめられます。自分で動かすリレーには上限がありません。
site-price-soon = 料金は近日公開
site-plan-free = Free
site-plan-pro = Pro
site-plan-team = Team
site-plan-free-1 = すべてのアプリのすべての機能
site-plan-free-2 = ホスト型リレーでルームを { $days } 日保存
site-plan-free-3 = 1 ルームあたり観覧リンク { $links } 件
site-plan-free-4 = 自分のリレーや LAN なら上限なし
site-plan-pro-1 = Free のすべて
site-plan-pro-2 = ホスト型リレーでルームを { $days } 日保存
site-plan-pro-3 = 自分の端末が所有するルームに適用
site-plan-team-1 = Pro のすべて
site-plan-team-2 = ホスト型リレーでルームを { $days } 日保存
site-plan-team-3 = チーム全員に適用
site-plan-team-4 = チームで 1 つの請求
site-plan-free-action = ダウンロード
site-plan-paid-action = アカウントを作成

## FAQ
site-faq-title = よくある質問
site-faq-q1 = Polyph はコードを読めますか？
site-faq-a1 = 読めません。コード、チャット、ボード、ターミナルは端末上で暗号化されます。リレーは暗号文を転送するだけで、鍵を持ちません。アカウントが記録するのは、誰がどの端末の料金を払うかだけです。
site-faq-q2 = アカウントは必要ですか？
site-faq-a2 = 不要です。Polyph はアカウントなしで、ホスト型リレー、自分のリレー、インターネットのない LAN で動きます。アカウントを使うと、ホスト型リレーでの保存期間が延び、チームで請求をまとめられます。
site-faq-q3 = どのエージェントが使えますか？
site-faq-a3 = Claude Code、Codex、OpenCode、ACP に対応したあらゆるエージェント、そして OpenAI 互換エンドポイント経由のローカルモデルです。契約とキーは自分のものを使います。Polyph がモデル呼び出しを中継することはありません。
site-faq-q4 = 2 つのエージェントが同じファイルを必要としたら？
site-faq-a4 = 先に確保した方が使います。後の方は誰が確保しているかを知らされ、待つか、別の作業をするか、質問します。関数単位で確保すれば、2 つのエージェントが 1 つのファイルの別々の部分を編集できます。
site-faq-q5 = リレーを自分で動かせますか？
site-faq-a5 = はい。SQLite を使う小さな単一バイナリです。負荷試験では、5 人のルーム 50 個でピーク { $mb } MB でした。

## Footer
site-footer-statement = Polyph のエンジンとアプリは Apache-2.0 のオープンソースです。ホスト型のアカウントサービスはプロプライエタリです。
site-footer-legal = © 2026 Polyph
site-footer-cta = Polyph を入手

## Download
site-dl-title = Polyph をインストール
site-dl-sub = 1 つのコマンドで、コマンドライン、デーモン、リレーをホームディレクトリにインストールします。root は不要です。ダウンロードはリリースのチェックサムで検証します。
site-dl-step-install = Linux と macOS
site-dl-step-install-win = Windows（PowerShell）
site-dl-source-title = またはソースからビルド
site-dl-releases = GitHub のすべてのダウンロード
site-dl-need = 必要なもの
site-dl-need-rust = Rust 1.94 以降
site-dl-need-git = git
site-dl-need-agent = いつも使っているコーディングエージェント、またはローカルモデル
site-dl-step-host = リポジトリでルームをホスト
site-dl-step-join = チームメイトがコードで参加
site-dl-immutable-title = イミュータブルな Linux
site-dl-immutable = Bazzite、Fedora Atomic などでは、cargo はホームディレクトリにインストールするため、読み取り専用のシステムには触れません。エージェントのサンドボックスは bubblewrap または Landlock を使い、どちらも root なしで使えます。
site-dl-relay-title = 自分のリレー
site-dl-relay = リレーは Docker と Caddy で、またはリリースアーカイブから systemd サービスとして動かします。暗号化されたスナップショットを 1 つの SQLite ファイルに保存します。
site-dl-apps-title = デスクトップアプリと VS Code
site-dl-apps = デスクトップアプリ（.dmg、.msi、.deb、.rpm、.AppImage）は polyph を内蔵しています。アプリと VS Code 拡張（.vsix）はリリースページにあります。

## Security
site-sec-title = リレーに見えるもの
site-sec-sub = リレーは端末間で暗号化されたエンベロープを転送します。リレーが知ることとその理由は、これですべてです。
site-sec-content = エンドツーエンドで暗号化
site-sec-content-text = コード、チャット、ボード、ホワイトボード、スレッド、メモ、エージェントのトランスクリプト、差分、ターミナル出力。鍵はメンバーの端末にだけあります。
site-sec-seen = 見えるもの
site-sec-seen-1 = ルーム ID、端末の公開鍵、接続時刻、IP アドレス（転送とレート制限のため）。
site-sec-seen-2 = エンベロープのヘッダー：ルーム、送信端末、エポック、シーケンス番号、サイズ。
site-sec-seen-3 = どの端末がどのルームにいるか。メンバー ID とロールはあるが名前はない。
site-sec-seen-4 = 誰もオンラインでないときに追いつくための、暗号化されたスナップショット。
site-sec-seen-5 = アカウントがある場合：メールアドレスまたは GitHub ID、プラン、自分の端末。
site-sec-cannot = できないこと
site-sec-cannot-1 = チームが書いたものを読むこと。
site-sec-cannot-2 = メッセージの偽造：すべてのエンベロープは送信端末が署名します。
site-sec-cannot-3 = メンバーの追加：参加にはコード全体とオーナーの承認が必要です。
site-sec-keys = モデルのキー
site-sec-keys-text = モデルの認証情報はマシンの外に出ません。Polyph はモデル呼び出しを中継せず、既定でテレメトリも送りません。
site-sec-docs = 脅威モデルの全文を読む

## Legal
site-legal-draft = 草案です。このページは法務レビューを経ておらず、まだ効力はありません。
site-privacy-title = プライバシー
site-privacy-h1 = ホスト型サービスが保持するもの
site-privacy-intro = Polyph は、チームが書いたものをサーバーが読めないように作られています。この草案では、ホスト型リレーとアカウントサービスが保持するものを説明します。
site-privacy-h-content = 読めないもの
site-privacy-content = ルームの内容はエンドツーエンドで暗号化されます。鍵を持たないため、誰にも渡せません。
site-privacy-h-relay = リレーが保持するもの
site-privacy-relay = 転送データ（ルーム ID、端末の公開鍵、接続時刻、IP アドレス、メッセージのサイズ）と暗号化されたスナップショット。スナップショットはプランの保存期間が過ぎると削除されます。
site-privacy-h-account = アカウントが保持するもの
site-privacy-account = メールアドレスまたは GitHub ID、言語、リンクした端末、チーム、プランと支払い状況。カード情報は決済代行会社が扱い、こちらには届きません。
site-privacy-h-delete = データの削除
site-privacy-delete = アカウントページでアカウントを削除できます。先にサブスクリプションを解約してください。その後、アカウント、セッション、リンクした端末、チームの所属を削除します。
site-terms-title = 利用規約
site-terms-h1 = ホスト型サービスの利用規約
site-terms-intro = この利用規約の草案は、ホスト型リレーとアカウントサービスを対象とします。オープンソースのエンジンとアプリは Apache License 2.0 に従います。
site-terms-h-use = サービスの利用
site-terms-use = ホスト型リレーは自分とチームの作業に使ってください。他者への攻撃、違法な内容の送信、過負荷をかける行為には使わないでください。
site-terms-h-content = あなたの内容
site-terms-content = ルームの内容はあなたのものです。暗号化されており、こちらは読めません。
site-terms-h-plans = プランと請求
site-terms-plans = 有料プランは解約するまで更新されます。料金は支払い前に表示します。解約しても、支払い済みの期間の終わりまでプランは続きます。
site-terms-h-changes = 変更
site-terms-changes = この規約を変更する場合は、適用前にお知らせします。

## Account
site-acct-title = アカウント
site-acct-h1 = Polyph アカウント
site-acct-sub = 任意です。アカウントを使うと、ホスト型リレーでのルーム保存期間が延び、チームで請求をまとめられます。Polyph はアカウントなしでも使えます。
site-acct-signin = サインイン
site-acct-email = メールアドレス
site-acct-send = コードをメールで送る
site-acct-code = メールに届いたコード
site-acct-code-sent = { $email } に 8 桁のコードを送りました。10 分間有効です。
site-acct-verify = サインイン
site-acct-github = GitHub でサインイン
site-acct-github-open = { $url } を開き、{ $code } を入力してください。承認するとこのページが続きます。
site-acct-or = または
site-acct-signout = サインアウト
site-acct-plan = プラン
site-acct-limits = ホスト型リレーでルームを { $days } 日保存します。
site-acct-limits-none = ホスト型リレーでの保存期間に上限はありません。
site-acct-devices = 端末
site-acct-devices-none = まだ端末がありません。コンピューターで polyph login を実行してリンクしてください。
site-acct-device-unnamed = 名前のない端末
site-acct-linked = { $date } にリンク
site-acct-unlink = リンク解除
site-acct-teams = チーム
site-acct-teams-none = チームに所属していません。
site-acct-team-name = チーム名
site-acct-team-create = チームを作成
site-acct-invite-email = チームメイトのメールアドレス
site-acct-invite = 招待
site-acct-invited = { $email } に招待を送りました。
site-acct-invite-accept = チームに招待されています。
site-acct-invite-join = チームに参加
site-acct-joined = { $team } に参加しました。
site-acct-role-owner = オーナー
site-acct-role-member = メンバー
site-acct-leave = 脱退
site-acct-billing = 請求
site-acct-billing-off = 請求はまだ利用できません。
site-acct-upgrade = { $plan } を選ぶ
site-acct-team-upgrade = { $team } に Team を選ぶ
site-acct-manage = 請求を管理
site-acct-billing-done = 支払いを受け付けました。まもなくプランが更新されます。
site-acct-delete = アカウントを削除
site-acct-delete-confirm = アカウントを削除しますか？リンクした端末とチームの所属も削除されます。元に戻せません。
site-acct-working = 処理中…
site-acct-error = 問題が発生しました：{ $detail }
site-acct-unreachable = アカウントサービスに接続できません。後でもう一度お試しください。
site-acct-err-wrong_code = コードが正しくありません。メールを確認して、もう一度お試しください。
site-acct-err-expired = 期限切れです。最初からやり直してください。
site-acct-err-invalid_email = メールアドレスではありません。
site-acct-err-rate_limited = 試行回数が多すぎます。しばらく待ってから、もう一度お試しください。
site-acct-err-owns_team = 先に、オーナーになっているチームを削除してください。
site-acct-err-subscribed = 先に「請求を管理」でサブスクリプションを解約してください。
site-acct-err-billing_not_configured = 請求はまだ利用できません。
site-acct-err-github_not_configured = GitHub でのサインインはまだ利用できません。メールアドレスを使ってください。
