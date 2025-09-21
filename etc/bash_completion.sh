# bash completion for upbuild                              -*- shell-script -*-

_upbuild()
{
  IFS=$'\n'
  local cur prev words cword split # needed by _init_completion()
  # Do not treat = as word breaks even if they are in $COMP_WORDBREAKS:
  # Split option=value into option in $prev and value in $cur
  _init_completion -s || return

  case $prev in
    '--ub-select'|'--ub-reject')
      # call upbuild to query tags
      readarray -t OPTS < <(upbuild --ub-completion-list-tags 2>&1 ; true)
      COMPREPLY=( $(compgen -W "${OPTS[*]}" -- $cur) )
      return 0
      ;;
  esac

  case $cur in
    -*)
      # GENERATE THESE ARGUMENTS
      OPTS=(--ub-print --ub-add --ub-no-env --ub-select= --ub-reject=)
      # suppress the space
      compopt -o nospace
      # add the space back in where needed
      for i in ${!OPTS[*]}; do [[ ${OPTS[$i]} == *= ]] || OPTS[$i]+=' ' ; done
      COMPREPLY=( $(compgen -W "${OPTS[*]}" -- "$cur") )
      return 0
      ;;
  esac

  compopt -o bashdefault -o default
  # completing with "" should go to bashdefault (normal completion)
  # could try to get fancy by looking for programs with -- for args?
  COMPREPLY=( $(compgen -W "" -- $cur) )
  return 0
}
complete -F _upbuild upbuild
