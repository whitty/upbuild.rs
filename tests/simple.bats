#!/usr/bin/env bats
#
# SPDX-License-Identifier: GPL-3.0-or-later
# (C) Copyright 2024 Greg Whiteley

setup_file() {
  if [ -z "$UPBUILD_OVERRIDE" ]; then
    # ensure we have up to date build
    cargo build
  fi
}

setup() {
  OLD_STYLE_ARGS_HANDLER=

  if [ -n "$UPBUILD_OVERRIDE" ]; then
    upbuild="$UPBUILD_OVERRIDE"
  else
    # ensure executable exists
    upbuild=$(readlink -f target/debug/upbuild)
    #rb_ref=1 # set this and upbuild above to wire in old rb version
    if [ -n "$rb_ref" ]; then
      OLD_STYLE_ARGS_HANDLER=true
    fi
  fi

  test -x "$upbuild"

  test_dir=$(mktemp -d)
  mkdir -p "${test_dir}"/1/1.1
  pushd "${test_dir}"
  cat > .upbuild <<EOF
echo
toplevel
&&
false
EOF

  mkdir -p "${test_dir}"/1/1.1
  cat > 1/1.1/.upbuild <<EOF
echo
--
1.1
&&
upbuild
EOF

  cat > 1/.upbuild <<EOF
echo
dir
--
1
&&
echo
@tags=on
2
&&
echo
@manual
@tags=on
3
EOF

}

teardown() {
  popd
  [ -d "$test_dir" ] && rm -rf "$test_dir"
}

@test "basic run" {
  cd 1

  run "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "dir 1
2" ]
}

@test "basic run --ub-print" {
  cd 1

  run "$upbuild" --ub-print
  [ "$status" -eq 0 ]
  [ "$output" = "echo dir 1
echo 2" ]
}

@test "basic run --ub-print args" {
  cd 1

  run "$upbuild" --ub-print 3
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "echo dir 3
echo 3" ]
  else
    [ "$output" = "echo dir 3
echo 2 3" ]
  fi
}

@test "basic run args" {
  cd 1

  run "$upbuild" 3
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "dir 3
3" ]
  else
    [ "$output" = "dir 3
2 3" ]
  fi
}

@test "basic run -- args" {
  cd 1

  run "$upbuild" -- --ub-print
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "dir --ub-print
--ub-print" ]
  else
    [ "$output" = "dir --ub-print
2 --ub-print" ]
fi
}

@test "run --" {
  cd 1

  run "$upbuild" --
  [ "$output" = "dir 1
2" ]
  [ "$status" -eq 0 ]
}

@test "run -- --" {
  cd 1

  run "$upbuild" -- --
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "dir --
--" ]
  else
    [ "$output" = "dir --
2 --" ]
  fi
}

@test "run ---" {
  cd 1

  run "$upbuild" ---
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "dir" ]
  else
    # --- isn't handled specially - just passed through as args
    [ "$output" = "dir ---
2 ---" ]
  fi
}

@test "recurse run" {
  cd 1/1.1

  run "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "1.1
upbuild: Entering directory \`$test_dir/1'
dir 1
2" ]
}

# recurse calls to shell, not actually recursing
@test "recurse run args" {
  cd 1/1.1

  run "$upbuild" 3
  [ "$status" -eq 0 ]
  if [ -n "$OLD_STYLE_ARGS_HANDLER" ]; then
    # replaces all
    [ "$output" = "3
upbuild: Entering directory \`$test_dir/1'
dir 3
3" ]
  else
    [ "$output" = "3
upbuild: Entering directory \`$test_dir/1'
dir 3
2 3" ]
  fi
}

@test "outfile" {
  mkdir 2
  cd 2
  cat > .upbuild <<EOF
echo
foo
@outfile=log.txt
EOF

  # Old rb version didn't fail here
  if [ -z "$rb_ref" ]; then
    run "$upbuild"
    [ "${lines[0]}" = "foo" ]
    echo "${lines[1]}" | grep -q "Unable to read @outfile=log.txt"
    [ "$status" -ne 0 ]
  fi

  echo bar > log.txt
  run "$upbuild"
  [ "$output" = "foo
bar" ]
  [ "$status" -eq 0 ]
}

@test "multi --" {
  mkdir 3
  cd 3
  cat > .upbuild <<EOF
ls
-la
--
--
--help
EOF

  run "$upbuild"
  echo "${lines[0]}" | grep -q -e "--help.*No such file or directory"
  [ "$status" -ne 0 ]
  output=""

  run "$upbuild" --ub-print
  [ "$output" = "ls -la -- --help" ]
  [ "$status" -eq 0 ]
}

@test "retmap" {
  mkdir 4
  cd 4
  cat > .upbuild <<EOF
sh
-c
@retmap=0=>1,1=>0,2=>4
--
exit 0
EOF

cat .upbuild

  run "$upbuild"
  [ "$status" -eq 1 ]

  run "$upbuild" "exit 0"
  [ "$status" -eq 1 ]

  run "$upbuild" "exit 1"
  [ "$status" -eq 0 ]

  run "$upbuild" "exit 2"
  [ "$status" -eq 4 ]
}

@test "find not local" {
  mkdir -p 1/2/3/4
  cd 1/2/3/4

  run "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory \`$test_dir/1'
dir 1
2" ]
}

@test "find not local - actual directory" {
  mkdir -p 1/2/3/4

  cat > 1/2/.upbuild <<EOF
pwd
EOF

  cd 1/2/3/4

  run "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory \`$test_dir/1/2'
$test_dir/1/2" ]
}

@test "cd in and out" {
  mkdir -p 1/2/3

  cat > 1/2/.upbuild <<EOF
pwd
&&
pwd
@cd=3
&&
pwd
EOF

  cd 1/2

  run "$upbuild"
  [ "$status" -eq 0 ]
  if [ -z "$rb_ref" ]; then
    [ "$output" = "$test_dir/1/2
upbuild: Entering directory \`$test_dir/1/2/3'
$test_dir/1/2/3
upbuild: Entering directory \`$test_dir/1/2'
$test_dir/1/2" ]
  else
    # Old rb version didn't report return back to original dir
    [ "$output" = "$test_dir/1/2
upbuild: Entering directory \`$test_dir/1/2/3'
$test_dir/1/2/3
$test_dir/1/2" ]
  fi
}

@test "cd in and out - relative" {
  mkdir -p 1/2/3

  cat > 1/.upbuild <<EOF
pwd
@cd=2
&&
pwd
@cd=2/3
EOF

  cd 1/2/3

  run "$upbuild"
  [ "$status" -eq 0 ]
  if [ -z "$rb_ref" ]; then
    [ "$output" = "upbuild: Entering directory \`$test_dir/1'
upbuild: Entering directory \`$test_dir/1/2'
$test_dir/1/2
upbuild: Entering directory \`$test_dir/1/2/3'
$test_dir/1/2/3" ]
  else
    # Old rb version didn't report return back to original dir
    [ "$output" = "upbuild: Entering directory \`$test_dir/1'
upbuild: Entering directory \`$test_dir/1/2'
$test_dir/1/2
upbuild: Entering directory \`$test_dir/1/2/3'
$test_dir/1/2/3" ]
  fi
}

@test "--ub-add" {
  mkdir -p 1/2
  cd 1/2

  [ ! -f .upbuild ]

  run "$upbuild" --ub-add pwd
  [ "$status" -eq 0 ]
  [ "$output" = "" ]

  content=$(cat .upbuild)
  [ "$content" = "pwd" ]

  run "$upbuild" --ub-add echo $(pwd)
  [ "$status" -eq 0 ]
  [ "$output" = "" ]

  content=$(cat .upbuild)
  [ "$content" = "pwd
&&
echo
$test_dir/1/2" ]
}
