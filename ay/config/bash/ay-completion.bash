# config/bash/ay-completion.bash: `ay` is clap-based without a bash generator.
# Installed by tools/shells.ts into the cache directory when an `ay` is on PATH.
declare -A _ay_help_cache 2>/dev/null
_ay_complete() {
  local cur=${COMP_WORDS[COMP_CWORD]} key=ay i line words='' in_cmds=0
  for ((i = 1; i < COMP_CWORD; i++)); do [[ ${COMP_WORDS[i]} == -* ]] || key+=" ${COMP_WORDS[i]}"; done
  if [[ -z ${_ay_help_cache[$key]+x} ]]; then
    while IFS= read -r line; do
      line=${line%$'\r'}
      case $line in
        Commands:) in_cmds=1; continue ;;
        '') in_cmds=0; continue ;;
      esac
      if ((in_cmds)) && [[ $line =~ ^\ \ ([a-z0-9][a-z0-9_-]*) ]]; then
        [[ ${BASH_REMATCH[1]} == help ]] || words+=" ${BASH_REMATCH[1]}"
      fi
      # clap option lines: "-h, --help" or "--check".
      if [[ $line =~ ^\ +(-[A-Za-z0-9]),\ (--[A-Za-z0-9][A-Za-z0-9-]*) ]]; then
        words+=" ${BASH_REMATCH[1]} ${BASH_REMATCH[2]}"
      elif [[ $line =~ ^\ +(--[A-Za-z0-9][A-Za-z0-9-]*) ]]; then
        words+=" ${BASH_REMATCH[1]}"
      fi
    done < <(command $key --help 2>/dev/null)
    _ay_help_cache[$key]=$words
  fi
  COMPREPLY=($(compgen -W "${_ay_help_cache[$key]}" -- "$cur"))
}
complete -o bashdefault -o default -F _ay_complete ay
