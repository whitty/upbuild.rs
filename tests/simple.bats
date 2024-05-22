#!/usr/bin/env bats

setup_file() {
  # ensure we have up to date build
  cargo build
}

setup() {
  # ensure executable exists
  upbuild=$(readlink -f target/debug/upbuild)
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
  [ "$output" = "1
2" ]
}

@test "basic run --ub-print" {
  cd 1

  run "$upbuild" --ub-print
  [ "$status" -eq 0 ]
  [ "$output" = "echo 1
echo 2" ]
}

@test "basic run --ub-print args" {
  cd 1

  run "$upbuild" --ub-print 3
  [ "$output" = "echo 3
echo 2 3" ]
  [ "$status" -eq 0 ]
}

@test "basic run args" {
  cd 1

  run "$upbuild" 3
  [ "$output" = "3
2 3" ]
  [ "$status" -eq 0 ]
}

@test "basic run -- args" {
  cd 1

  run "$upbuild" -- --ub-print
  [ "$output" = "--ub-print
2 --ub-print" ]
  [ "$status" -eq 0 ]
}

@test "recurse run" {
  cd 1/1.1

  run "$upbuild"
  [ "$status" -eq 0 ]
  [ "$output" = "1.1
1
2" ]
}

# recurse calls to shell, not actually recursing
@test "recurse run args" {
  cd 1/1.1

  run "$upbuild" 3
  [ "$status" -eq 0 ]
  [ "$output" = "3
3
2 3" ]
}

@test "outfile" {
  mkdir 2
  cd 2
  cat > .upbuild <<EOF
echo
foo
@outfile=log.txt
EOF

  run "$upbuild"
  [ "${lines[0]}" = "foo" ]
  echo "${lines[1]}" | grep -q "Unable to read @outfile=log.txt"
  [ "$status" -ne 0 ]

  echo bar > log.txt
  run "$upbuild"
  [ "$output" = "foo
bar" ]
  [ "$status" -eq 0 ]
}
