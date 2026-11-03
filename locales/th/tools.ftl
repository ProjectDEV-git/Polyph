tool-read = อ่าน { $count } ไฟล์ ({ $files })
tool-search = ค้นหา { $query } (พบ { $matches } รายการ)
tool-search-plain = ค้นหา { $query }
tool-search-many = ค้นหา { $count } ครั้ง
tool-edit = แก้ไข { $file } (+{ $added } −{ $removed })
tool-delete = ลบ { $files }
tool-move = ย้าย { $files }
tool-run = รัน { $command }
tool-run-tests = รัน { $command }: ผ่าน { $passed } ไม่ผ่าน { $failed }
tool-run-failed = รัน { $command } (รหัสออก { $code })
tool-think = คิดทบทวน
tool-fetch = { $count ->
    [one] ดึง { $target }
   *[other] ดึงหน้าเว็บ { $count } หน้า
}
tool-mcp = ใช้ { $tool }
tool-other = { $title }
