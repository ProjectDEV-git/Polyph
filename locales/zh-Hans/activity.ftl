# 活动动态条目。每行旁边会显示执行者的名字和颜色。
activity-card-created = 在 { $column } 中创建了 { $title }
activity-card-moved = 将 { $title } 从 { $from } 移到了 { $to }
activity-card-updated = 编辑了 { $title }
activity-card-removed = 删除了 { $title }
activity-merged = 将 { $branch } 合并到了 { $into }
activity-merge-failed = 无法将 { $branch } 合并到 { $into }
activity-changes-requested = 评审要求修改 { $branch }
activity-agent-started = 启动了一个 { $kind } 智能体
activity-agent-stopped = 停止了一个智能体
activity-approval = { $approved ->
    [true] 批准了 { $title }
   *[other] 拒绝了 { $title }
}
activity-overlap = { $agent } 修改了 { $path }，该文件由 { $holder } 认领
activity-thread-created = 发起了讨论串 { $title }
activity-whiteboard-created = 创建了白板 { $name }
activity-terminal-opened = 打开了终端 { $title }
