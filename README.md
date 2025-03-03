# Upbuild.rs

`upbuild-rs` is a rust reimplementation of my hacky integration helper
`upbuild` as seen [here](https://github.com/whitty/upbuild).  The
remainder of this README comes from version 0.8.0 of the `rb` version
and should exist as an aspirational product definition.

# Upbuild

Simple directory tree build helper

## Usage

Write a command-file named `.upbuild` in the directory at the level you
want to build.  In basic usage build command line is created by
passing each line of the file as an argument.  Eg:

    echo
    hello
    world

When run with `upbuild` will run echo "hello" "world" from the shell.
Upbuild looks back toward the root of your directory tree until it
finds a `.upbuild` file to run.  The directory that the command-file
is found in becomes the working directory for the command defined in
the file.

### Passing arguments from command-line

You can break a command into mandatory and overridable parts by
splitting it using "`--`".  eg:

    ls
    -la
    --
    some_directory

When run as `upbuild` will run `ls -la some_directory`.  However if
you instead run as `upbuild another_directory` it will run `ls -la
another_directory`.  The part of the command after `--` will be
replaced with the arguments to `upbuild`.

If you need to pass -- you can do so after the -- interpreted by build.

    ls
    -la
    --
    --
    --help

to produce a `-l` listing of the file name `--help`

### Multiple commands

Additionally multiple commands can be strung-together by separating
them using `&&`.  Each command will be run as long until one command
returns a failure, or the last command is run.  The return-code for
the command will be that of the last command (ie the failure, or if
all successful, the last command).  eg:

    make
    TARGET=debug
    --
    tests
    &&
    make
    TARGET=release
    --
    tests

When invoked as `upbuild` will run `make TARGET=debug tests`, and if
that succeeds run `make TARGET=release tests`.  If you want to publish
both you could build a target other than tests by specifying it on the
command-line.  eg: `upbuild publish`.

### Argument parsing and `--`

On the command `--` works like other GNU command parsing, no further
interpretation of commands is performed by `upbuild`.

To invoke commands with just the mandatory parts you need to pass an
argument in (but don't want it to take effect), so for a sub-command
that takes `--` you could use:

```
upbuild -- --
```

With the second being passed to the command, thus discarding the
overridable parameters.  Of course not every commands supports `--`,
so `upbuild` allows allows the argument `---` to mean 'like `--`, but
also truncate the command to just the mandatory parts'.

```
upbuild ---
```

### Getting output from GUI commands

Some build tools are GUI focused and don't nicely support
scripting. Some such tools may have a silent "build feature, but no
build feed-back.  Thankfully some of these generate their own output
files, so we may synthesise some output.

    uv4
    @outfile log.txt
    -j0
    -b
    project.uvproj
    -o log.txt

The following build will execute "uv4 -j0 -b project.uvproj -o
log.txt" and emit the contents of log.txt at the end of the run -
irrespective of success or failure.

### Fixing odd error codes

Some build tools return error codes that may not represent an error.
Use the option `@retmap` to provide a comma separated list of
return-code mappings - integer=>integer.

    uv4
    # uv4 returns 1 if errors occurred - our library includes
    # suck so map 1 to a success
    @retmap=1=>0
    -j0
    -b
    project.uvproj
    -o log.txt

The following build will execute "uv4 -j0 -b project.uvproj -o
log.txt" as above, but return-value of 1 will be mapped to success (0)

### Printing commands

Print the commands that would be executed, but don't execute them
using --ub-print.

## Advanced usage

### Controlling execution

Sometimes you need to exclude a command from a list - mark it as
`@disable`.

    make
    tests
    &&
    make
    @disable
    install

Or you can add tags to allow later selection of subsets.  For example:

    make
    @tags=host
    tests
    &&
    make
    @tags=target
    cross
    &&
    make
    @tags=release,host
    install

When run as `upbuild` all commands will run - select a subset using
`--ub-select=<tag>`.  Eg running `upbuild --ub-select=host` would
exclude the `make cross` command.

Alternatively you can use `--ub-reject=<tag>` to exclude based on
tags.   Eg running `upbuild --ub-reject=host` would
only run the `make cross` command.  When both select and reject are
specified - tags must both match the selected tag, and not match the
rejected tag.

If both reject and select refer to the same tag, whichever command is
specified *last* will take effect.

To prevent a command being run unless a `@tag` is specifically selected mark it `@manual`.  Running the following without parameters won't run the `make install` step, but selecting `release` or `host` will:

```
make
@tags=host
tests
&&
make
@tags=target
cross
&&
make
@manual
@tags=release,host
install
```

### Recursive calls

If the command being invoked is `upbuild` itself it will be invoked from
the next level down.  You can use this to layer your calls, or provide
scoping.

    $ cat ../.upbuild
    make
    link_only
    $ cat .upbuild
    make
    component1
    component1_tests
    &&
    upbuild

Invoking `upbuild` in the component1 directory will build the local
component, then pass up the chain for the next action - relinking.

Combine this with `--ub-select` to rebuild a single tag based on your
location.  For example assuming the `.upbuild` file from the @tags
example, you may have the following in a target sub-directory:

    $ cat src/target/.upbuild
    upbuild
    --ub-select=target

Builds under src/target will only invoke commands tagged with
'target'.

### Changing directory

You can use the `@cd` directive to run the command from the specified
directory.

    $ cat .upbuild
    run_from_specific_directory
    @cd=/path/to/that/specific/directory

This is most useful if you need to "shell-out" to a different upbuild
tree:

    $ cat .upbuild
    make
    -j8
    &&
    upbuild
    @cd=/path/to/the/rest

### Creating a directory

You can use the `@mkdir` directive to request that a directory be created if it does not exist before running the command.

Currently the `@mkdir` target is evaluated relative to the execution directory _before_ handling `@cd`.

I use this workflow to help with `cmake`:

    cmake
    ..
    @cd=build
    @mkdir=build
    @tags=fresh
    @manual
    --fresh
    &&
    cmake
    @cd=build
    --build
    .

To rerun `cmake` itself run `upbuild --ub-select=fresh`


### Quickly adding new commands

Use `--ub-add` to quickly add commands to the .upbuild file

    $ upbuild --ub-add make -j8
    $ upbuild --ub-add make test
    $ cat .upbuild
    make
    -j8
    &&
    make
    test

This can be handy if you want to resolve shell expansions into the
file - ie:

    $ upbuild --ub-add ls ~
    $ cat .upbuild
    ls
    /home/user
