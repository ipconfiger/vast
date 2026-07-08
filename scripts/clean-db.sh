#!/usr/bin/env bash
# clean-db.sh — 删除开发数据库和上传文件，方便重新测试
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

echo "🧹 清理开发数据库和上传文件..."

# 数据库在 target/{debug,release}/data/im.db
for build_dir in target/debug/data target/release/data; do
    if [ -d "$build_dir" ]; then
        # 删除 SQLite 数据库
        rm -f "$build_dir"/im.db "$build_dir"/im.db-wal "$build_dir"/im.db-shm
        echo "   已删除: $build_dir/im.db*"
        # 删除上传文件
        if [ -d "$build_dir/uploads" ]; then
            rm -rf "$build_dir/uploads"
            echo "   已删除: $build_dir/uploads/"
        fi
    fi
done

echo ""
echo "✅ 清理完成，可以重新运行 dev-server"
