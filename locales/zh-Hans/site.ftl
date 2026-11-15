# The website (apps/web): marketing pages, pricing, download, security,
# legal drafts and the account page.

## Shared
site-title = Polyph：让团队的编程智能体同处一个房间
site-description = Polyph 是面向人类与其智能体团队的多人协作编码环境，适用于 Linux、macOS 和 Windows。
site-nav-label = 网站
site-nav-home = Polyph 首页
site-nav-pricing = 价格
site-nav-download = 下载
site-nav-security = 安全
site-nav-account = 账户
site-nav-privacy = 隐私（草稿）
site-nav-terms = 条款（草稿）
site-nav-source = 源代码
site-lang-label = 语言
site-theme-dark = 深色
site-theme-light = 浅色
site-theme-label = 主题
site-skip = 跳到正文

## Home: hero
site-hero-title = 团队的编程智能体，同处一个房间。
site-hero-sub = 每个人在自己的机器上运行自己的智能体。Polyph 让它们看到彼此的工作、编辑前先认领文件，并在合并时互不冲突。
site-hero-facts-label = 概览
site-fact-platforms = Linux（包括 Bazzite 和 Fedora Atomic）；macOS 和 Windows 处于试验阶段。
site-fact-e2e = 端到端加密：中继只转发密文
site-fact-selfhost = 中继可自行托管，只是一个小型二进制文件
site-fact-agents = 任何智能体都能用，使用你自己的密钥
site-fact-langs = English、ไทย、日本語、简体中文
site-rail-agents = Claude Code、Codex、OpenCode、ACP 智能体和本地模型
site-rail-code = 用 8 位代码加入
site-rail-lan = 在没有互联网的局域网中也能使用
site-rail-open = 引擎和应用采用 Apache-2.0
site-rail-action = 下载 Polyph

## Interface fragments (sample room)
site-ui-room = payments
site-ui-pill-running = { $count } 个运行中
site-ui-pill-waiting = { $count } 个等待批准
site-ui-agent-of = { $name } 的智能体
site-ui-line-1 = 编辑前已认领 src/pay/refund.ts。
site-ui-line-2 = refund.ts 已被 { $name } 的智能体认领。改为处理 webhook 处理程序。
site-ui-line-3 = 测试通过。提交任务供审查。
site-ui-line-4 = 退款超时要重试吗？卡片上写的是要。
site-ui-line-5 = 两者都已合并：超时重试和幂等键。
site-ui-approve-title = { $name } 的智能体请求运行命令
site-ui-approve-cmd = 命令
site-ui-approve-files = 文件
site-ui-approve-hosts = 网络
site-ui-approve-none = 无
site-ui-approve-once = 仅批准一次
site-ui-approve-task = 为此任务批准
site-ui-deny = 拒绝并附说明
site-ui-status-running = 运行中
site-ui-status-waiting = 等待批准
site-ui-status-blocked = 受阻
site-ui-status-done = 已完成
site-ui-claimed-by = 由 { $name } 认领
site-ui-knock = { $name } 请求加入
site-ui-approve = 批准
site-ui-merged = 已合并到 main
site-ui-kept-both = 保留了两者的意图
site-ui-col-todo = 待办
site-ui-col-doing = 进行中
site-ui-col-review = 审查
site-ui-card-1 = 退款超时重试
site-ui-card-2 = 退款幂等键
site-ui-card-3 = webhook 签名校验

## Home: live room
site-show-title = 设置一个房间
site-show-sub = 选择团队，看看谁负责什么。每个人有一种颜色；他们的智能体使用同一颜色更浅和更深的色阶，分配方式与 Polyph 完全一致。
site-show-controls = 房间设置
site-show-people = 人数
site-show-agents = 每人的智能体
site-show-claims = 认领
site-show-approvals = 运行命令前询问
site-show-theme = 浅色主题
site-show-mode = 共享工作副本
site-show-fewer = 减少
site-show-more = 增加
site-show-roster = 房间成员
site-show-mode-distributed = 每个任务有自己的 worktree
site-show-mode-hosted = 主机上的一个共享工作副本
site-show-claims-off = 认领已关闭：智能体可能编辑同一个文件。
site-show-next = 然后在你的仓库中运行 polyph host，分享它显示的代码。
site-show-link = 工作方式

## Home: how it works
site-how-title = 从代码到合并
site-how-host = 主持
site-how-host-text = 在你的仓库中创建房间。Polyph 会显示加入代码。
site-how-join = 加入
site-how-join-text = 队友输入代码。在他们看到任何内容之前，你逐一批准。
site-how-plan = 规划与认领
site-how-plan-text = 规划器把目标拆成卡片。智能体编辑前先认领文件，所以不会有两个智能体写同一个文件。
site-how-merge = 审查与合并
site-how-merge-text = 审查智能体检查每个任务。冲突的合并方式是保留两个任务各自想做的事。

# The pillar strings below need a human translation review.
## Home: close-ups
site-parts-title = Polyph 核心功能
site-part-editing = 多人智能体实时并行编辑
site-part-editing-text = 多名开发者及其智能体同时工作：编辑前先认领文件、文件夹或函数，每个任务在自己的 worktree 中进行。
site-part-safety = 安全与隐私控制
site-part-safety-text = 中继无法读取的端到端加密房间、按智能体隔离的沙箱、标明确切命令、文件和主机的审批门，以及公开的数据保留期限。
site-part-server = 本地 CLI 服务器
site-part-server-text = 你自己的守护进程通过 WebSocket 共享房间、打印加入码并提供浏览器客户端。局域网无需互联网即可使用。
site-part-router = 内置模型路由器
site-part-router-text = 从 small、standard、frontier 三档中为每个智能体任务挑选模型，预算紧张时自动降档。
site-fig-tiers = 3 档
site-part-models = 灵活的模型支持
site-part-models-text = 支持 Claude Code、Codex、Gemini CLI、OpenCode、任意 ACP 智能体与本地模型，使用你自己的订阅和密钥。
site-fig-models = ACP
site-part-context = 团队全域共享上下文
site-part-context-text = 所有会话共享同一房间状态：看板、钉在代码上的笔记、智能体收件箱与交接，实时同步。
site-part-langs = 四种语言
site-part-langs-text = 每个界面和命令都支持英语、泰语、日语和中文。

## Home: surfaces
site-surfaces-title = 所有界面
site-surfaces-count = { $count } 个界面，底层是同一个引擎。
site-surface-open = 了解更多
site-surface-cli = 命令行
site-surface-cli-text = 每个操作都是一条命令：主持、加入、启动智能体、批准、规划和合并。脚本可以读取 JSON。
site-surface-tui = 终端界面
site-surface-tui-text = 不带参数运行 polyph，即可查看聊天、看板、智能体和批准。可通过 SSH 使用。
site-surface-desktop = 桌面应用
site-surface-desktop-text = 整个房间就在编辑器旁边，带托盘图标和系统通知。
site-surface-browser = 浏览器
site-surface-browser-text = 同一个应用在浏览器标签页中运行，通过一次性链接从你自己的机器打开。
site-surface-phone = 手机
site-surface-phone-text = 在手机上关注房间、批准智能体，从浏览器安装。
site-surface-vscode = VS Code
site-surface-vscode-text = 在打开的文件上显示认领，批准以通知形式出现。
site-surface-pill = 状态胶囊
site-surface-pill-text = 一个置顶的小窗口，可以就地回应第一个批准请求。
site-surface-everywhere = Linux、macOS、Windows
site-surface-phones = iOS 和 Android 浏览器
site-surface-editor = 所有桌面系统上的 VS Code

## Marquee
site-marquee = 独立的声音，同一个房间

## Pricing
site-pricing-title = 套餐
site-pricing-h1 = 托管中继的套餐
site-pricing-sub = 不注册账户也能使用 Polyph。套餐提供托管中继上更长的房间保留期和统一账单。你自己运行的中继没有任何限制。
site-price-soon = 价格即将公布
site-plan-free = Free
site-plan-pro = Pro
site-plan-team = Team
site-plan-free-1 = 所有应用的全部功能
site-plan-free-2 = 托管中继上房间保留 { $days } 天
site-plan-free-3 = 每个房间 { $links } 个旁观链接
site-plan-free-4 = 自己的中继或局域网，不受限制
site-plan-pro-1 = 包含 Free 的全部内容
site-plan-pro-2 = 托管中继上房间保留 { $days } 天
site-plan-pro-3 = 适用于你自己的设备拥有的房间
site-plan-team-1 = 包含 Pro 的全部内容
site-plan-team-2 = 托管中继上房间保留 { $days } 天
site-plan-team-3 = 适用于团队中的每个人
site-plan-team-4 = 团队一张账单
site-plan-free-action = 下载
site-plan-paid-action = 创建账户

## FAQ
site-faq-title = 常见问题
site-faq-q1 = Polyph 能读取我们的代码吗？
site-faq-a1 = 不能。代码、聊天、看板和终端都在你的设备上加密。中继只转发密文，从不持有密钥。账户只记录谁为哪些设备付费。
site-faq-q2 = 需要注册账户吗？
site-faq-a2 = 不需要。不注册也能在托管中继、你自己的中继或没有互联网的局域网上使用 Polyph。账户提供托管中继上更长的房间保留期和团队账单。
site-faq-q3 = 支持哪些智能体？
site-faq-a3 = Claude Code、Codex、OpenCode、任何支持 ACP 的智能体，以及通过 OpenAI 兼容接口使用的本地模型。使用你自己的订阅和密钥；Polyph 从不代理模型调用。
site-faq-q4 = 两个智能体需要同一个文件时会怎样？
site-faq-a4 = 先到的智能体认领它。后到的会被告知谁持有它，然后等待、转做别的，或者询问。对单个函数的认领可以让两个智能体编辑同一文件的不同部分。
site-faq-q5 = 可以自己运行中继吗？
site-faq-a5 = 可以。它是一个使用 SQLite 的小型二进制文件。在我们的负载测试中，50 个房间、每个房间 5 人时峰值为 { $mb } MB。

## Footer
site-footer-statement = Polyph 的引擎和应用以 Apache-2.0 开源。托管账户服务为专有软件。
site-footer-legal = © 2026 Polyph
site-footer-cta = 获取 Polyph

## Download
site-dl-title = 安装 Polyph
site-dl-sub = 一条命令即可把命令行、守护进程和中继安装到你的主目录，无需 root，并根据发行版的校验和验证下载内容。
site-dl-step-install = Linux 和 macOS
site-dl-step-install-win = Windows（PowerShell）
site-dl-source-title = 或从源代码构建
site-dl-releases = GitHub 上的全部下载
site-dl-need = 你需要
site-dl-need-rust = Rust 1.94 或更高版本
site-dl-need-git = git
site-dl-need-agent = 你已经在用的编程智能体，或本地模型
site-dl-step-host = 在你的仓库中主持房间
site-dl-step-join = 队友用代码加入
site-dl-immutable-title = 不可变 Linux
site-dl-immutable = 在 Bazzite、Fedora Atomic 等系统上，cargo 安装到你的主目录，不会触及只读系统。智能体沙箱使用 bubblewrap 或 Landlock，两者都不需要 root。
site-dl-relay-title = 你自己的中继
site-dl-relay = 用 Docker 和 Caddy 运行中继，或从发行版压缩包以 systemd 服务运行。它把加密快照保存在一个 SQLite 文件中。
site-dl-apps-title = 桌面应用和 VS Code
site-dl-apps = 桌面应用（.dmg、.msi、.deb、.rpm、.AppImage）自带 polyph。桌面应用和 VS Code 扩展（.vsix）都在发行版页面上。

## Security
site-sec-title = 中继能看到什么
site-sec-sub = 中继在你的设备之间转发加密的信封。以下是它获知的全部内容及原因。
site-sec-content = 端到端加密
site-sec-content-text = 代码、聊天、看板、白板、讨论串、记忆笔记、智能体记录、差异和终端输出。密钥只存在于成员的设备上。
site-sec-seen = 它能看到的
site-sec-seen-1 = 房间 ID、设备公钥、连接时间和 IP 地址，用于路由和限流。
site-sec-seen-2 = 信封头：房间、发送设备、纪元、序号和大小。
site-sec-seen-3 = 哪些设备在哪些房间中，带成员 ID 和角色，但没有名字。
site-sec-seen-4 = 加密快照，以便没人在线时也能补上进度。
site-sec-seen-5 = 有账户时：你的邮箱或 GitHub ID、套餐，以及哪些设备属于你。
site-sec-cannot = 它不能做的
site-sec-cannot-1 = 读取团队写下的任何内容。
site-sec-cannot-2 = 伪造消息：每个信封都由发送设备签名。
site-sec-cannot-3 = 添加成员：加入需要完整的代码和所有者的批准。
site-sec-keys = 你的模型密钥
site-sec-keys-text = 模型凭据从不离开你的机器。Polyph 从不代理模型调用，默认也不发送遥测数据。
site-sec-docs = 阅读完整的威胁模型

## Legal
site-legal-draft = 草稿。本页尚未经过法律审核，目前不具效力。
site-privacy-title = 隐私
site-privacy-h1 = 托管服务保留哪些数据
site-privacy-intro = Polyph 的设计让服务器无法读取团队写下的内容。本草稿说明托管中继和账户服务会保留什么。
site-privacy-h-content = 我们无法读取的
site-privacy-content = 房间内容采用端到端加密。我们没有密钥，也无法把内容交给任何人。
site-privacy-h-relay = 中继保留的
site-privacy-relay = 路由数据（房间 ID、设备公钥、连接时间、IP 地址和消息大小）以及加密快照。快照在套餐的保留期结束后过期。
site-privacy-h-account = 账户保留的
site-privacy-account = 你的邮箱或 GitHub ID、语言、关联的设备、团队，以及套餐和付款状态。银行卡信息由支付服务商处理，不会到达我们这里。
site-privacy-h-delete = 删除你的数据
site-privacy-delete = 在账户页面删除账户。请先取消订阅；之后我们会删除你的账户、会话、关联设备和团队成员身份。
site-terms-title = 条款
site-terms-h1 = 托管服务条款
site-terms-intro = 本条款草稿适用于托管中继和账户服务。开源的引擎和应用适用 Apache License 2.0。
site-terms-h-use = 使用服务
site-terms-use = 将托管中继用于你自己和团队的工作。不要用它攻击他人、发送违法内容或使其过载。
site-terms-h-content = 你的内容
site-terms-content = 房间的内容属于你。它是加密的，我们无法读取。
site-terms-h-plans = 套餐和账单
site-terms-plans = 付费套餐在取消前会自动续订。付款前会显示价格。取消后，套餐会保留到已付费周期结束。
site-terms-h-changes = 变更
site-terms-changes = 本条款如有变更，我们会在生效前公告。

## Account
site-acct-title = 账户
site-acct-h1 = 你的 Polyph 账户
site-acct-sub = 可选。账户提供托管中继上更长的房间保留期和团队账单。不注册也能使用 Polyph。
site-acct-signin = 登录
site-acct-email = 邮箱地址
site-acct-send = 通过邮件发送登录码
site-acct-code = 邮件中的登录码
site-acct-code-sent = 我们已向 { $email } 发送 8 位登录码，10 分钟内有效。
site-acct-verify = 登录
site-acct-github = 使用 GitHub 登录
site-acct-github-open = 打开 { $url } 并输入 { $code }。你批准后，本页会继续。
site-acct-or = 或
site-acct-signout = 退出登录
site-acct-plan = 套餐
site-acct-limits = 托管中继上房间保留 { $days } 天。
site-acct-limits-none = 托管中继上的保留期不受限制。
site-acct-devices = 设备
site-acct-devices-none = 还没有设备。在电脑上运行 polyph login 即可关联。
site-acct-device-unnamed = 未命名设备
site-acct-linked = 关联于 { $date }
site-acct-unlink = 取消关联
site-acct-teams = 团队
site-acct-teams-none = 你还没有加入任何团队。
site-acct-team-name = 团队名称
site-acct-team-create = 创建团队
site-acct-invite-email = 队友的邮箱
site-acct-invite = 邀请
site-acct-invited = 已向 { $email } 发送邀请。
site-acct-invite-accept = 你被邀请加入一个团队。
site-acct-invite-join = 加入团队
site-acct-joined = 你已加入 { $team }。
site-acct-role-owner = 所有者
site-acct-role-member = 成员
site-acct-leave = 退出
site-acct-billing = 账单
site-acct-billing-off = 账单功能尚未开放。
site-acct-upgrade = 选择 { $plan }
site-acct-team-upgrade = 为 { $team } 选择 Team
site-acct-manage = 管理账单
site-acct-billing-done = 已收到付款。你的套餐稍后会更新。
site-acct-delete = 删除账户
site-acct-delete-confirm = 要删除你的账户吗？关联的设备和团队成员身份都会被移除，且无法撤销。
site-acct-working = 处理中…
site-acct-error = 出现问题：{ $detail }
site-acct-unreachable = 无法连接账户服务。请稍后再试。
site-acct-err-wrong_code = 登录码不正确。请检查邮件后重试。
site-acct-err-expired = 已过期。请重新开始。
site-acct-err-invalid_email = 这不是有效的邮箱地址。
site-acct-err-rate_limited = 尝试次数过多。请稍后再试。
site-acct-err-owns_team = 请先删除你拥有的团队。
site-acct-err-subscribed = 请先在“管理账单”中取消订阅。
site-acct-err-billing_not_configured = 账单功能尚未开放。
site-acct-err-github_not_configured = 暂不支持 GitHub 登录。请使用邮箱。
