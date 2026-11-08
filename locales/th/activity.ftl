# รายการในฟีดกิจกรรม ชื่อและสีของผู้ทำจะแสดงข้างแต่ละบรรทัด
activity-card-created = สร้าง { $title } ใน { $column }
activity-card-moved = ย้าย { $title } จาก { $from } ไป { $to }
activity-card-updated = แก้ไข { $title }
activity-card-removed = ลบ { $title }
activity-merged = รวม { $branch } เข้า { $into }
activity-merge-failed = รวม { $branch } เข้า { $into } ไม่สำเร็จ
activity-changes-requested = ผู้รีวิวขอให้แก้ไข { $branch }
activity-agent-started = เริ่มเอเจนต์ { $kind }
activity-agent-stopped = หยุดเอเจนต์
activity-approval = { $approved ->
    [true] อนุมัติ { $title } ให้ { $agent }
   *[other] ปฏิเสธ { $title } ของ { $agent }
}
activity-overlap = { $agent } แก้ไข { $path } ซึ่ง { $holder } จองไว้
activity-thread-created = เริ่มเธรด { $title }
activity-whiteboard-created = สร้างไวต์บอร์ด { $name }
activity-terminal-opened = เปิดเทอร์มินัล { $title }
activity-agent-connected = เชื่อมต่อ { $client }
activity-task-finished = ทำงานเสร็จ: { $summary }
activity-brief-updated = แก้ไขสรุปโปรเจกต์
activity-task-taken = รับงาน { $title }
activity-certificates-reissued = ออกใบรับรองใหม่ให้ { $names } (บทบาทสูงสุดคือสมาชิก)
