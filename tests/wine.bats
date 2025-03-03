#!/usr/bin/env bats
#
# SPDX-License-Identifier: GPL-3.0-or-later
# (C) Copyright 2024 Greg Whiteley

## TODO - merge with simple??

export WINEDEBUG=-all
export WINEARCH=win64
export WINEPREFIX=$(readlink -f "${BATS_TEST_DIRNAME}/../.wine")
target=${target:-x86_64-pc-windows-gnu}
wine=${wine:-wine}

setup_file() {
  # Ensure wine doesn't emit extra stuff on first run
  env DISPLAY= "$wine" cmd /c dir 2> /dev/null > /dev/null

  # ensure we have up to date build
  cargo build --target $target > /dev/null
}

setup() {
  # grrr - old bats doesn't support setup_file?
  if [ ! -f "target/${target}/debug/upbuild.exe" ]; then
    setup_file
  fi

  # ensure executable exists
  upbuild=$(readlink -f "target/${target}/debug/upbuild.exe")

  test -r "$upbuild"

  test_dir=$(mktemp -d)
  mkdir -p "${test_dir}"/1/1.1
  pushd "${test_dir}"
  cat > .upbuild <<EOF
cmd
/c
echo
toplevel
&&
false
EOF

  mkdir -p "${test_dir}"/1/1.1
  cat > 1/1.1/.upbuild <<EOF
cmd
/c
echo
--
1.1
&&
upbuild
EOF

  cat > 1/.upbuild <<EOF
cmd
/c
echo
dir
--
1
&&
cmd
/c
echo
@tags=on
2
&&
cmd
/c
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

run_win() {
  run "$wine" "$@"
  output=$(echo "$output" | tr -d "\r")
  # This monstrosity should re-split output into lines[]
  # it return failure on EOF of output (hence ||true)
  IFS="
" read -d '' -a lines <<<"${output}" || true
}

@test "${target} basic run" {
  cd 1

  run_win $upbuild
  [ "$status" -eq 0 ]
  [ "$output" = "dir 1
2" ]
}

@test "${target} basic run --ub-print" {
  cd 1

  run_win "$upbuild" --ub-print
  [ "$status" -eq 0 ]
  [ "$output" = "cmd /c echo dir 1
cmd /c echo 2" ]
}

@test "${target} basic run --ub-print args" {
  cd 1

  run_win "$upbuild" --ub-print 3
  [ "$status" -eq 0 ]
  [ "$output" = "cmd /c echo dir 3
cmd /c echo 2 3" ]
}

@test "${target} basic run args" {
  cd 1

  run_win "$upbuild" 3
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

@test "${target} basic run -- args" {
  cd 1

  run_win "$upbuild" -- --ub-print
  [ "$status" -eq 0 ]
  [ "$output" = "dir --ub-print
2 --ub-print" ]
}

@test "${target} run --" {
  cd 1

  run_win "$upbuild" --
  [ "$output" = "dir 1
2" ]
  [ "$status" -eq 0 ]
}

@test "${target} run -- --" {
  cd 1

  run_win "$upbuild" -- --
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

@test "${target} run ---" {
  cd 1

  run_win "$upbuild" ---
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

convert_dir() {
  echo "$(winepath -w "$1")"
}

display_dir() {
  echo "\`\\\\?\\$(convert_dir "$1")'"
}

# 'straight' ticks instead of `make' ticks
display_straight_dir() {
  echo "'\\\\?\\$(convert_dir "$1")'"
}

@test "${target} recurse run" {
  cd 1/1.1

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "1.1
upbuild: Entering directory $(display_dir ${test_dir}/1)
dir 1
2" ]
}

# recurse calls to shell, not actually recursing
@test "${target} recurse run args" {
  cd 1/1.1

  run_win "$upbuild" 3
  [ "$status" -eq 0 ]
  [ "$output" = "3
upbuild: Entering directory $(display_dir ${test_dir}/1)
dir 3
2 3" ]
}

@test "${target} outfile" {
  mkdir 2
  cd 2
  cat > .upbuild <<EOF
cmd
/c
echo
foo
@outfile=log.txt
EOF

  run_win "$upbuild"
  [ "${lines[0]}" = "foo" ]
  echo "${lines[1]}" | grep -q "Unable to read @outfile=log.txt"
  [ "$status" -ne 0 ]

  echo bar > log.txt
  run_win "$upbuild"
  [ "$output" = "foo
bar" ]
  [ "$status" -eq 0 ]
}

@test "${target} multi --" {
  mkdir 3
  cd 3
  cat > .upbuild <<EOF
cmd
/c
dir
--
--
--help
EOF

  run_win "$upbuild"
  echo "${output}" | grep -q -e "File not found"
  [ "$status" -ne 0 ]
  output=""

  run_win "$upbuild" --ub-print
  [ "$output" = "cmd /c dir -- --help" ]
  [ "$status" -eq 0 ]
}

@test "${target} retmap" {
  mkdir 4
  cd 4
  cat > .upbuild <<EOF
cmd
/c
@retmap=0=>1,1=>0,2=>4
--
exit 0
EOF

cat .upbuild

  run_win "$upbuild"
  [ "$status" -eq 1 ]

  run_win "$upbuild" "exit 0"
  [ "$status" -eq 1 ]

  run_win "$upbuild" "exit 1"
  [ "$status" -eq 0 ]

  run_win "$upbuild" "exit 2"
  [ "$status" -eq 4 ]
}

@test "${target} find not local" {
  mkdir -p 1/2/3/4
  cd 1/2/3/4

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir}/1)
dir 1
2" ]

  run_win "$upbuild" --ub-print
  [ "$status" -eq 0 ]
  [ "$output" = "rem cd $(display_straight_dir ${test_dir}/1)
cmd /c echo dir 1
cmd /c echo 2" ]
}

@test "${target} find not local - actual directory" {
  mkdir -p 1/2/3/4

  cat > 1/2/.upbuild <<EOF
cmd
/c
cd
EOF

  cd 1/2/3/4

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir}/1/2)
$(convert_dir ${test_dir}/1/2)" ]
}

@test "${target} cd in and out" {
  mkdir -p 1/2/3

  cat > 1/2/.upbuild <<EOF
cmd
/c
cd
&&
cmd
/c
cd
@cd=3
&&
cmd
/c
cd
EOF

  cd 1/2

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "$(convert_dir ${test_dir}/1/2)
upbuild: Entering directory $(display_dir ${test_dir}/1/2/3)
$(convert_dir ${test_dir}/1/2/3)
upbuild: Entering directory $(display_dir ${test_dir}/1/2)
$(convert_dir ${test_dir}/1/2)" ]
}

@test "${target} cd in and out - relative" {
  mkdir -p 1/2/3

  cat > 1/.upbuild <<EOF
cmd
/c
cd
@cd=2
&&
cmd
/c
cd
@cd=2/3
EOF

  cd 1/2/3

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir}/1)
upbuild: Entering directory $(display_dir ${test_dir}/1/2)
$(convert_dir ${test_dir}/1/2)
upbuild: Entering directory $(display_dir ${test_dir}/1/2/3)
$(convert_dir ${test_dir}/1/2/3)" ]
}

@test "${target} --ub-add" {
  mkdir -p 1/2
  cd 1/2

  [ ! -f .upbuild ]

  run_win "$upbuild" --ub-add pwd
  [ "$status" -eq 0 ]
  [ "$output" = "" ]

  content=$(cat .upbuild)
  [ "$content" = "pwd" ]

  run_win "$upbuild" --ub-add echo $(pwd)
  [ "$status" -eq 0 ]
  [ "$output" = "" ]

  content=$(cat .upbuild)
  [ "$content" = "pwd
&&
echo
$test_dir/1/2" ]
}

@test "${target} @mkdir" {

  ! test -f build
  ! test -d build

    cat > .upbuild <<EOF
cmd
@cd=build
@mkdir=build
/c
cd
&&
cmd
@cd=build
/c
cd
EOF

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir}/build)
$(convert_dir ${test_dir}/build)
$(convert_dir ${test_dir}/build)" ]

  # should have created build
  test -d build

  rmdir build

  # now there's a file in place
  touch build
  ! test -d build
  test -f build

  run_win "$upbuild"
  [ "$status" -ne 0 ]
  echo "${output}" | grep -q "ailed to create directory build"
}

@test "${target} @mkdir non-local" {

  ! test -f build
  ! test -d build

    cat > .upbuild <<EOF
cmd
@cd=build
@mkdir=build
/c
cd
&&
cmd
@cd=build
/c
cd
EOF

  d=$(pwd)
  mkdir -p once/twice
  cd once/twice

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir})
upbuild: Entering directory $(display_dir ${test_dir}/build)
$(convert_dir ${test_dir}/build)
$(convert_dir ${test_dir}/build)" ]

  cd "${d}"

  # should have created build
  test -d build

  rmdir build

  # now there's a file in place
  touch build
  ! test -d build
  test -f build

  cd once/twice

  run_win "$upbuild"
  [ "$status" -ne 0 ]
  echo "${output}" | grep -q "ailed to create directory .*build"
}

@test "${target} @mkdir - levels" {

  ! test -f build
  ! test -d build

    cat > .upbuild <<EOF
cmd
@cd=build/2
@mkdir=build/2
/c
cd
&&
cmd
@cd=build/2
/c
cd
EOF

  run_win "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "upbuild: Entering directory $(display_dir ${test_dir}/build/2)
$(convert_dir ${test_dir}/build/2)
$(convert_dir ${test_dir}/build/2)" ]

  # should have created build
  test -d build/2

  rmdir build/2

  # now there's a file in place
  touch build/2
  ! test -d build/2
  test -f build/2

  run_win "$upbuild"
  [ "$status" -ne 0 ]
  echo "${output}" | grep -q "ailed to create directory build/2"
}
