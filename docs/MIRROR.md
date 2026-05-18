# Gitea → GitHub Auto-Mirror

C2-Tracker dùng **Gitea Push Mirror** (native, version 1.18+) để tự động đẩy commits + tags từ self-hosted Gitea (`git.microai.club`) sang public GitHub mirror (`github.com/taipm/C2-Tracker`).

## Vì sao có 2 mirrors

| Mirror | Vai trò |
|---|---|
| **Gitea** `git.microai.club/taipm/C2-Tracker` | Source-of-truth nội bộ (CI/CD, issues, private branches) |
| **GitHub** `github.com/taipm/C2-Tracker` | Public access cho one-line installer (`curl -fsSL ... | bash`) |

Gitea instance bật `REQUIRE_SIGNIN_VIEW` → anonymous curl bị 401. GitHub mirror cho phép cài đặt anonymous giống Claude Code.

## Workflow

```
Developer push lên Gitea (origin)
        ↓
Gitea push mirror — sync_on_commit=true, interval=10m fallback
        ↓
GitHub repository public (read-only mirror)
        ↓
End user: curl -fsSL .../install.sh | bash → tải release từ GitHub
```

- **sync_on_commit**: mỗi commit push lên Gitea kích sync ngay (~3-5 giây).
- **interval 10 phút**: fallback nếu sync_on_commit fail vì lý do mạng.
- **last_error** field track lỗi sync gần nhất qua API.

## Setup ban đầu (1 lần)

### 1. Tạo GitHub repo

```bash
gh repo create taipm/C2-Tracker --public \
  --description "Realtime recording studio cho Claude Code" \
  --homepage "https://git.microai.club/taipm/C2-Tracker"
```

### 2. Tạo GitHub Personal Access Token (PAT)

- Truy cập https://github.com/settings/tokens
- Scope cần: `repo` (Full control of private repositories) — Gitea cần để force-push branches/tags.
- Sao chép token, dùng ở bước 3.

### 3. Tạo Gitea Push Mirror

Qua Gitea Web UI:
- Repo → Settings → Mirror Settings → "Push Mirror" → "Add"
- Git remote: `https://github.com/taipm/C2-Tracker.git`
- Authorization: `Username: taipm`, `Password: <github-pat>`
- Sync interval: `10m`
- Tick **Sync on commit**

Hoặc qua API:

```bash
GH_TOKEN=$(gh auth token)  # hoặc PAT đã tạo
curl -n -X POST -H "Content-Type: application/json" \
  https://git.microai.club/api/v1/repos/taipm/C2-Tracker/push_mirrors \
  -d @- << EOF
{
  "remote_address": "https://github.com/taipm/C2-Tracker.git",
  "remote_username": "taipm",
  "remote_password": "$GH_TOKEN",
  "interval": "10m0s",
  "sync_on_commit": true
}
EOF
```

### 4. Push initial content

```bash
# Push code + tags từ Gitea → GitHub (lần đầu, force)
git remote add github https://github.com/taipm/C2-Tracker.git
git push github main
git push github --tags
```

### 5. Tạo GitHub release

```bash
gh release create v0.0.1 \
  --repo taipm/C2-Tracker \
  --title "v0.0.1 — MVP Phase 1 (2025-06-18)" \
  --notes-file CHANGELOG.md \
  c2-tracker-v0.0.1-darwin-arm64.tar.gz
```

## Workflow thường ngày

Sau khi setup xong, developer **chỉ push lên Gitea**:

```bash
git push origin main
# Tự động sync sang GitHub trong vài giây
```

**Không thêm github remote local** — tránh nhầm lẫn push 2 lần.

## Verify sync hoạt động

```bash
# Test 1 commit
echo "test" > /tmp/sync-test && git add . && git commit -m "test sync"
git push origin main

# Đợi 5 giây
sleep 5

# Check GitHub có commit mới chưa
gh api repos/taipm/C2-Tracker/commits --jq '.[0].sha'
git rev-parse HEAD
# 2 SHA phải khớp
```

## Troubleshooting

| Triệu chứng | Nguyên nhân | Cách fix |
|---|---|---|
| `last_error: authentication failed` | GitHub PAT expired/revoked | Tạo PAT mới, update mirror qua API `PATCH /push_mirrors/{name}` hoặc UI |
| Sync lag > 10 phút | `sync_on_commit` bị tắt | Verify qua API GET, bật lại |
| Tags không sync | Mirror config thiếu `--tags` | Gitea push mirror tự động bao gồm tags — nếu thiếu, force push tay |
| Release artifact không sync | Push mirror chỉ sync git objects, không sync releases/issues | Tạo release thủ công trên GitHub qua `gh release create` |

## Limitations

- **Issues** không sync (chỉ git data). Issues Gitea ≠ Issues GitHub.
- **Releases** không sync. Mỗi version release phải `gh release create` thủ công.
- **Webhooks, settings** không sync.
- **Push mirror là one-way**: GitHub → Gitea KHÔNG sync ngược. PR/issue/edit trên GitHub sẽ bị overwrite lần sync sau.

## Quản lý mirror

```bash
# List mirrors
curl -s -n https://git.microai.club/api/v1/repos/taipm/C2-Tracker/push_mirrors | jq

# Trigger sync ngay (không đợi)
curl -X POST -n https://git.microai.club/api/v1/repos/taipm/C2-Tracker/push_mirrors-sync

# Xoá mirror (cần remote_name từ list)
curl -X DELETE -n \
  https://git.microai.club/api/v1/repos/taipm/C2-Tracker/push_mirrors/<remote_name>
```
