# AgenTeam 记忆功能第五轮独立复验（Windows）

2026-11-23。基线为 main 的 5e0bab967d90c5c2ecead3ae90cfa8ed13dcfa42，报告分支为 acceptance/memory-reverify-4。只运行 Windows 上的指定脚本和 W5 三项测试文件。使用合成 USER.md 与隔离的 NTFS 临时目录，产品脚本和测试源码均按检出版本运行。

先读了 acceptance/memory-reverify-3 的 REVERIFY-3.md 中 S1-B、S1-C、S1-E，以及主线 specs/tech_docs/assistant_profile.md 的「第四轮复验后」一节。

## W1–W5 结果

| 用例 | 结果 | 证据 |
| --- | --- | --- |
| W1（S1-C） | 失败 | 3.7.1 无读取 40/40、有读取 20/20。3.12.13 无读取 40/40、有读取 9/20。共 11 轮失败、13 个写入进程退出 5。 |
| W2（S1-E） | 通过 | 两种 Python × 两种锁状态，4/4；目录拒绝创建经实测，所有返回为退出 5，目标哈希不变。 |
| W3（S1-B） | 通过 | 同名和异名每种 Python 各 20 轮，80/80；真实路径持锁测试 4/4。 |
| W4 | 通过 | 持锁时返回 LOCK_BUSY，终止持锁进程后首次写入成功，2/2。 |
| W5 | 失败 | 指定命令的首次默认环境为 30 通过、9 失败、3 跳过，退出 1。UTF-8 复跑为 3.7.1 37 通过、2 失败、3 跳过，退出 1；3.12.13 38 通过、1 失败、3 跳过，退出 1。 |

## 环境与原版脚本

- Windows 11 26H2 x64，10.0.26300.9974，NTFS，Node.js v24.12.0。
- CPython 3.7.1，conda-forge，MSC v.1900 AMD64。CPython 3.12.13，Anaconda，MSC v.1942 AMD64。W1–W4 显式指定解释器，并设置 PYTHONUTF8=1、PYTHONIOENCODING=utf-8 以捕获完整诊断。
- 普通终端没有管理员令牌，系统未配置开发者模式，最初 os.symlink 返回 WinError 1314。随后用 Windows PowerShell 的 RunAs 启动管理员终端，只在隔离目录内创建四个文件符号链接，帮助进程退出 0。W3 的写入仍在普通权限下运行。
- 四份磁盘副本的 SHA-256 均为 a4eeac57cd40e06a7a5b1046df32e01f431473ca065e03f82ed770fefb8725fd。main 基线与 03e077c 逐路径 Git blob 均为 b524fa5698ea15136d475c2d7599ca741966282b。原版执行副本为 agenteam-memory-update。
  - bundled-skills/agenteam-memory-gardener/scripts/section_edit.py
  - bundled-skills/agenteam-memory-update/scripts/section_edit.py
  - bundled-skills/agenteam-proactive/scripts/section_edit.py
  - bundled-skills/agenteam-user-profile/scripts/section_edit.py

## W1 并发与读取压力

每轮先写入相同的 CRLF 合成基线，SHA-256 为 dc0e72fc9c2974d415e8d324814220c8fcae2bcf3f6c0bde11fc2bb382fd10e5。五个写入进程分别设置不同且实际存在的 TEMP 和 TMP，准备完成后由同一个文件信号释放。每进程以 runpy 将检出的原版脚本作为 __main__ 执行，传入相同的 --base-sha 和不同的作者标记，无代码注入或 monkeypatch。每轮按退出码、最终字节与唯一成功者核对。

压力组在释放写入者前由独立进程打开并读取目标，释放后首次持有约 300 ms，再循环打开、读取、持有约 12 ms、关闭并间隔约 4 ms。独立的共享验证在另一个文件上持有相同读句柄时直接执行 os.replace，两组均收到 WinError 5。读者完成次数见下，每轮保留读者退出码及完整输出。

600 个写入进程退出码分布为 0=120, 2=467, 5=13。所有 120 轮均只有一次成功，最终目标字节均与唯一成功者的预期匹配。退出 5 仍违反 W1 每轮的退出码要求。

逐轮记号为「按 writer-0 到 writer-4 顺序的退出码 / 最终 SHA-256 代号 / 成功读取次数」。无读取组使用短横线。下表列出所有轮次。

| 轮 | 3.7.1 无读取 | 3.7.1 循环读取 | 3.12.13 无读取 | 3.12.13 循环读取 |
| --- | --- | --- | --- | --- |
| 1 | 2,2,0,2,2 / H2 / — | 2,2,2,0,2 / H3 / 33 | 0,2,2,2,2 / H0 / — | 0,5,5,2,2 / H0 / 358 |
| 2 | 0,2,2,2,2 / H0 / — | 2,2,0,2,2 / H2 / 15 | 2,0,2,2,2 / H1 / — | 2,2,2,0,5 / H3 / 167 |
| 3 | 0,2,2,2,2 / H0 / — | 2,2,0,2,2 / H2 / 30 | 2,2,0,2,2 / H2 / — | 0,5,2,2,2 / H0 / 170 |
| 4 | 2,2,2,2,0 / H4 / — | 2,2,2,0,2 / H3 / 19 | 2,2,0,2,2 / H2 / — | 2,2,2,2,0 / H4 / 122 |
| 5 | 2,2,2,2,0 / H4 / — | 2,0,2,2,2 / H1 / 25 | 0,2,2,2,2 / H0 / — | 2,5,0,2,2 / H2 / 170 |
| 6 | 2,2,0,2,2 / H2 / — | 2,0,2,2,2 / H1 / 26 | 0,2,2,2,2 / H0 / — | 2,0,2,2,2 / H1 / 114 |
| 7 | 2,2,2,0,2 / H3 / — | 2,2,2,2,0 / H4 / 21 | 2,2,2,2,0 / H4 / — | 2,2,2,2,0 / H4 / 78 |
| 8 | 2,2,0,2,2 / H2 / — | 2,2,2,0,2 / H3 / 33 | 2,2,2,0,2 / H3 / — | 2,0,2,2,2 / H1 / 10 |
| 9 | 0,2,2,2,2 / H0 / — | 2,2,2,2,0 / H4 / 21 | 2,2,2,2,0 / H4 / — | 2,0,2,2,2 / H1 / 89 |
| 10 | 0,2,2,2,2 / H0 / — | 2,2,2,0,2 / H3 / 25 | 2,2,0,2,2 / H2 / — | 0,2,2,5,2 / H0 / 191 |
| 11 | 2,2,2,0,2 / H3 / — | 2,0,2,2,2 / H1 / 18 | 2,2,0,2,2 / H2 / — | 5,2,0,2,2 / H2 / 212 |
| 12 | 2,2,0,2,2 / H2 / — | 2,2,0,2,2 / H2 / 18 | 0,2,2,2,2 / H0 / — | 2,2,2,2,0 / H4 / 140 |
| 13 | 2,0,2,2,2 / H1 / — | 2,2,0,2,2 / H2 / 26 | 2,0,2,2,2 / H1 / — | 2,2,2,0,2 / H3 / 87 |
| 14 | 0,2,2,2,2 / H0 / — | 2,2,0,2,2 / H2 / 19 | 2,2,2,0,2 / H3 / — | 2,0,2,5,2 / H1 / 182 |
| 15 | 2,2,0,2,2 / H2 / — | 2,2,2,2,0 / H4 / 22 | 2,0,2,2,2 / H1 / — | 0,2,5,2,2 / H0 / 254 |
| 16 | 2,2,2,0,2 / H3 / — | 2,2,0,2,2 / H2 / 41 | 2,2,2,2,0 / H4 / — | 0,5,2,2,2 / H0 / 167 |
| 17 | 2,2,0,2,2 / H2 / — | 2,2,2,0,2 / H3 / 19 | 2,2,0,2,2 / H2 / — | 5,2,2,0,2 / H3 / 301 |
| 18 | 0,2,2,2,2 / H0 / — | 2,0,2,2,2 / H1 / 51 | 2,0,2,2,2 / H1 / — | 5,5,0,2,2 / H2 / 366 |
| 19 | 2,0,2,2,2 / H1 / — | 2,2,2,0,2 / H3 / 36 | 2,2,2,0,2 / H3 / — | 0,2,2,2,2 / H0 / 89 |
| 20 | 2,2,2,2,0 / H4 / — | 0,2,2,2,2 / H0 / 40 | 0,2,2,2,2 / H0 / — | 0,2,2,2,2 / H0 / 36 |
| 21 | 2,2,2,2,0 / H4 / — | — | 0,2,2,2,2 / H0 / — | — |
| 22 | 0,2,2,2,2 / H0 / — | — | 2,2,2,0,2 / H3 / — | — |
| 23 | 2,2,0,2,2 / H2 / — | — | 2,2,0,2,2 / H2 / — | — |
| 24 | 2,2,2,0,2 / H3 / — | — | 2,2,0,2,2 / H2 / — | — |
| 25 | 2,2,2,0,2 / H3 / — | — | 2,2,2,2,0 / H4 / — | — |
| 26 | 2,2,0,2,2 / H2 / — | — | 2,0,2,2,2 / H1 / — | — |
| 27 | 2,0,2,2,2 / H1 / — | — | 2,0,2,2,2 / H1 / — | — |
| 28 | 2,2,2,2,0 / H4 / — | — | 2,0,2,2,2 / H1 / — | — |
| 29 | 2,2,2,2,0 / H4 / — | — | 2,0,2,2,2 / H1 / — | — |
| 30 | 2,2,2,0,2 / H3 / — | — | 2,2,2,0,2 / H3 / — | — |
| 31 | 2,2,2,0,2 / H3 / — | — | 2,2,0,2,2 / H2 / — | — |
| 32 | 2,0,2,2,2 / H1 / — | — | 2,0,2,2,2 / H1 / — | — |
| 33 | 2,2,2,0,2 / H3 / — | — | 2,0,2,2,2 / H1 / — | — |
| 34 | 2,2,0,2,2 / H2 / — | — | 0,2,2,2,2 / H0 / — | — |
| 35 | 2,2,2,0,2 / H3 / — | — | 0,2,2,2,2 / H0 / — | — |
| 36 | 2,2,2,0,2 / H3 / — | — | 2,2,2,0,2 / H3 / — | — |
| 37 | 2,2,2,2,0 / H4 / — | — | 2,2,0,2,2 / H2 / — | — |
| 38 | 2,0,2,2,2 / H1 / — | — | 2,0,2,2,2 / H1 / — | — |
| 39 | 2,2,2,0,2 / H3 / — | — | 0,2,2,2,2 / H0 / — | — |
| 40 | 2,2,2,2,0 / H4 / — | — | 0,2,2,2,2 / H0 / — | — |

最终 SHA-256 代号如下。W3 同样使用此对照。

```text
H0 4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c
H1 44e7c60b7200ed1f7d027601c95c06d5844cda6497b7207e08bb70ec86468af7
H2 9c09b986fe3b097fe50386627a3725af8ecd669e6fce68fcf00e081a2c956533
H3 6a8d2c17be4d75c5f9972a7b0192bb7aaecd12995a0bd9735856ac75881ffa96
H4 e9763a469724969fd81afe751ad82f608ddc1b9e668ca03de9513e828b2ca889
```

### 全部退出 5 的进程输出

以下逐一保留 stdout、stderr、进程退出码和最终目标 SHA-256。字符串以 JSON 转义保存，唯一编辑是把本机绝对临时目录前缀替换成 <RUN>。原始未脱敏字节对应末尾 JSONL 的哈希。

```jsonl
{"python": "3.12.13", "pressure": true, "round": 1, "writer": "writer-1", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-01\\\\\\\\.USER.md.efe0576968b9.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-01\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 1, "writer": "writer-2", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-01\\\\\\\\.USER.md.983ae9a7f3ec.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-01\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 2, "writer": "writer-4", "exit": 5, "target_sha256": "6a8d2c17be4d75c5f9972a7b0192bb7aaecd12995a0bd9735856ac75881ffa96", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-02\\\\\\\\.USER.md.349266d41bb5.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-02\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 3, "writer": "writer-1", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-03\\\\\\\\.USER.md.6d7b262ca509.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-03\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 5, "writer": "writer-1", "exit": 5, "target_sha256": "9c09b986fe3b097fe50386627a3725af8ecd669e6fce68fcf00e081a2c956533", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-05\\\\\\\\.USER.md.4fe595751e6d.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-05\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 10, "writer": "writer-3", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-10\\\\\\\\.USER.md.d701fcfc7328.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-10\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 11, "writer": "writer-0", "exit": 5, "target_sha256": "9c09b986fe3b097fe50386627a3725af8ecd669e6fce68fcf00e081a2c956533", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-11\\\\\\\\.USER.md.37ee1607639c.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-11\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 14, "writer": "writer-3", "exit": 5, "target_sha256": "44e7c60b7200ed1f7d027601c95c06d5844cda6497b7207e08bb70ec86468af7", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-14\\\\\\\\.USER.md.2c0e74087d2a.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-14\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 15, "writer": "writer-2", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-15\\\\\\\\.USER.md.220d8ce27cca.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-15\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 16, "writer": "writer-1", "exit": 5, "target_sha256": "4945ec39a8e762c4e57d5419f6c119d5619eb484b5159ccddf8877c0835be41c", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-16\\\\\\\\.USER.md.0f1cfccffddc.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-16\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 17, "writer": "writer-0", "exit": 5, "target_sha256": "6a8d2c17be4d75c5f9972a7b0192bb7aaecd12995a0bd9735856ac75881ffa96", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-17\\\\\\\\.USER.md.c87059758c04.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-17\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 18, "writer": "writer-0", "exit": 5, "target_sha256": "9c09b986fe3b097fe50386627a3725af8ecd669e6fce68fcf00e081a2c956533", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-18\\\\\\\\.USER.md.3edc2bb2bb88.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-18\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
{"python": "3.12.13", "pressure": true, "round": 18, "writer": "writer-1", "exit": 5, "target_sha256": "9c09b986fe3b097fe50386627a3725af8ecd669e6fce68fcf00e081a2c956533", "stdout": "{\"ok\": false, \"code\": \"UNREADABLE\", \"message\": \"[WinError 5] 拒绝访问。: '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-18\\\\\\\\.USER.md.64b499b536a8.tmp' -> '<RUN>\\\\\\\\cases-W1-py31213\\\\\\\\reader-18\\\\\\\\USER.md'\"}\r\n", "stderr": ""}
```

## W2 与 W4

W2 先对夹具目录的当前用户 ACL 拒绝 FILE_ADD_FILE（icacls /deny 的 WD），该 ACE 不向文件继承。用读写句柄打开现有目标，另尝试创建文件并确认 PermissionError。测试后移除所设的拒绝 ACE，四次清理命令均退出 0。基线与以下每次最终哈希相同。

| Python | 旁置锁 | 退出码 | 耗时（秒） | 错误位置 | 最终 SHA-256 |
| --- | --- | --- | --- | --- | --- |
| 3.7.1 | 已存在 | 5 | 0.093 | 同目录临时文件 | dc0e72fc9c2974d415e8d324814220c8fcae2bcf3f6c0bde11fc2bb382fd10e5 |
| 3.7.1 | 不存在 | 5 | 0.062 | .USER.md.lock | dc0e72fc9c2974d415e8d324814220c8fcae2bcf3f6c0bde11fc2bb382fd10e5 |
| 3.12.13 | 已存在 | 5 | 0.078 | 同目录临时文件 | dc0e72fc9c2974d415e8d324814220c8fcae2bcf3f6c0bde11fc2bb382fd10e5 |
| 3.12.13 | 不存在 | 5 | 0.078 | .USER.md.lock | dc0e72fc9c2974d415e8d324814220c8fcae2bcf3f6c0bde11fc2bb382fd10e5 |

W4 独立持锁进程用 msvcrt.locking 锁住真实旁置文件的第一个字节。SECTION_EDIT_LOCK_WAIT=1，等待返回时检查持锁进程仍存活。随后调用 Python Popen.kill（Windows TerminateProcess），等待该进程退出，再发起写入。持锁期间目标 SHA-256 始终为基线。

| Python | 等待退出码 | 等待（秒） | 持锁者终止退出码 | 终止后写入退出码 | 写入（秒） | 最终 SHA-256 |
| --- | --- | --- | --- | --- | --- | --- |
| 3.7.1 | 2 | 1.110 | 1 | 0 | 0.094 | efa0d629a1e587687c5f735aa6a1b3b534bccecd9cc5bd3c06c0f75c231d27e1 |
| 3.12.13 | 2 | 1.063 | 1 | 0 | 0.063 | efa0d629a1e587687c5f735aa6a1b3b534bccecd9cc5bd3c06c0f75c231d27e1 |

## W3 文件符号链接

同名链接为另一目录下的 USER.md，异名链接为 LINK.md，二者指向同一真实 USER.md。每轮恢复真实文件基线，经真实路径和链接各启动一个写入者。核对唯一胜方、真实文件完整字节、链接类型与目标。表内顺序为「真实路径退出码、链接路径退出码 / 最终 SHA-256 代号」。每轮链接类型与指向均保持。

| 轮 | 3.7.1 同名 | 3.7.1 异名 | 3.12.13 同名 | 3.12.13 异名 |
| --- | --- | --- | --- | --- |
| 1 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 | 0,2 / H0 |
| 2 | 2,0 / H1 | 0,2 / H0 | 0,2 / H0 | 2,0 / H1 |
| 3 | 2,0 / H1 | 2,0 / H1 | 2,0 / H1 | 0,2 / H0 |
| 4 | 2,0 / H1 | 2,0 / H1 | 2,0 / H1 | 0,2 / H0 |
| 5 | 2,0 / H1 | 2,0 / H1 | 0,2 / H0 | 2,0 / H1 |
| 6 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 | 0,2 / H0 |
| 7 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 |
| 8 | 0,2 / H0 | 0,2 / H0 | 2,0 / H1 | 2,0 / H1 |
| 9 | 2,0 / H1 | 0,2 / H0 | 2,0 / H1 | 2,0 / H1 |
| 10 | 0,2 / H0 | 2,0 / H1 | 2,0 / H1 | 2,0 / H1 |
| 11 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 | 2,0 / H1 |
| 12 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 |
| 13 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 |
| 14 | 0,2 / H0 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 |
| 15 | 0,2 / H0 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 |
| 16 | 2,0 / H1 | 0,2 / H0 | 0,2 / H0 | 0,2 / H0 |
| 17 | 2,0 / H1 | 0,2 / H0 | 2,0 / H1 | 2,0 / H1 |
| 18 | 0,2 / H0 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 |
| 19 | 2,0 / H1 | 2,0 / H1 | 0,2 / H0 | 0,2 / H0 |
| 20 | 0,2 / H0 | 2,0 / H1 | 0,2 / H0 | 2,0 / H1 |

真实路径锁被持有时，四次链接路径写入均退出 2，stdout 返回 LOCK_BUSY，目标哈希保持基线。

| Python | 链接变体 | 退出码 | 耗时（秒） | 链接仍在 |
| --- | --- | --- | --- | --- |
| 3.7.1 | 同名 | 2 | 1.765 | 是 |
| 3.7.1 | 异名 | 2 | 3.484 | 是 |
| 3.12.13 | 同名 | 2 | 2.312 | 是 |
| 3.12.13 | 异名 | 2 | 2.343 | 是 |

## W5、缺陷和阻塞项

```text
node --test scripts/section-edit.test.mjs scripts/memory-skills.test.mjs scripts/proactive-skill.test.mjs
```

首次直接运行使用 PATH 上的 Python 3.7.1 与默认输出编码。9 个失败里，7 个报告中文输出解码不匹配，另有 1 个持锁夹具清理失败和 1 个 mkstemp 回归夹具退出码不符。显式 UTF-8 的两次复跑完整结果见表。三个跳过项是脚本在 Windows 上主动跳过的规则枚举符号链接用例及两条 section-edit 符号链接用例。W3 的独立手测已执行。

- UTF-8 / 3.7.1 失败测试
  - a writer that cannot get the lock fails with LOCK_BUSY and leaves the file alone
  - a directory we may not write to fails at once instead of retrying
- UTF-8 / 3.12.13 失败测试
  - a writer that cannot get the lock fails with LOCK_BUSY and leaves the file alone

- **S1-C 仍可复现。** CPython 3.12.13 在持续读取组仍会在同目录临时文件替换 USER.md 时返回 UNREADABLE / WinError 5。受影响的轮次均有唯一成功者，目标字节与胜方预期一致。
- **W5 Windows 测试夹具缺陷。** 持锁用例在目录清理钩子的 rmSync 报 EPERM。用例的清理钩子先尝试移除目录，后注册的 holder.kill 尚未运行。W4 在等待持锁进程实际退出后验证了内核释放。3.7.1 的 mkstemp 模拟夹具把 Python 函数赋给 os.open。该版本的 pathlib._accessor.open 将它绑定为方法，转发给原生 open 时多出一个参数，stderr 为 TypeError: open() takes at most 3 positional arguments (4 given)，子进程退出 1。相同夹具在 3.12.13 返回预期的退出 5。两种版本的实际 NTFS ACL 用例 W2 均通过。
- **阻塞项。** 无。W3 初始权限阻塞通过管理员终端解决。全量 scripts/*.test.mjs 不在本轮范围。
- **结论。** W2、W3、W4 通过。W1 未达到 S1-C 的通过条件，W5 未全通过。本轮整体判定不通过。

## 原始证据 SHA-256

文件保存在本机隔离复验目录，报告中的逐轮矩阵可直接核对全部写入退出码。JSONL 记录独立进程的完整输出、超时、基线和最终 SHA-256。首次默认编码 Node 运行的计数来自其终端摘要，下面的两份 W5 日志是 UTF-8 复跑原件。

| 文件 | SHA-256 |
| --- | --- |
| evidence-W1-py31213.jsonl | 16b3bd2e38f11069a1aaf96168562b2dd842e6beae195daed078bb6705b26a36 |
| evidence-W1-py371.jsonl | 77e922189f6e5463670673609ab807fbcf40dbd215cb4a16f27db606c71a3fda |
| evidence-W2-py31213.jsonl | 17533c6246e627df69c2acbeb0f6c9179892b08ac904e0fbeb76eb607e420fe3 |
| evidence-W2-py371.jsonl | 77ddf7c87be362c928f74a030731e7fab6043d0d17eec7e06361d788cb0116e6 |
| evidence-W3-py31213-elevated.jsonl | e3242f24ab5091e05244ddaae7cff5b08a26861cd487c33cf5e79ccd8349077f |
| evidence-W3-py31213.jsonl | daed5db497b872f1d65dfd789796dc4a633e5380071afc0858cf0db85430c71e |
| evidence-W3-py371-elevated.jsonl | 3fbb8d13531f693225914a492ceb7badd837561cae7b56c5b854e6e7ae700ecf |
| evidence-W3-py371.jsonl | 038fd869e358b066a917df7f262ce7d8b363b8a83fa4497ee66a2d0c27d50f49 |
| evidence-W4-py31213.jsonl | 7ca29dcbb5c3fb4aa43f0f0880912ccb267ecdabde9670d43c017124d3439bee |
| evidence-W4-py371.jsonl | 2cdd044614b28e891aa35f3174e169388d6a0951db6f89bc2857510b9303b5f9 |
| W5-utf8-py31213.log | f4a238de157146b356c112135ba151e32e3382825e0ec6a803eb091a46382e94 |
| W5-utf8-py371.log | 095ecc070c231fb88415420d9a34f810ec31ac718cb49f82572c2b28a474e361 |
| driver.py | 9a01e5629221ece57b9f976e4f46b843c6c216f514cd09194a0aa7e9f98c2b43 |
| driver-w3-elevated-fixtures.py | 27f454359823f3106d9ef08ee8d33558cf90d782054e3d1f94d432a270d6f8ac |
| W3-elevated.done | 1e832a85ec13f87cfde16644bb7b1046f95ba0e2c255e0534fdd41bf6002672c |
| W5-mkstemp-diagnostic.json | 0657ffe3f3c0083e80840e70b2b78bd8c00321b80239dec2edc6e2f7dcb80880 |
