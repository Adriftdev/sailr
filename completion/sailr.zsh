#compdef sailr

autoload -U is-at-least

_sailr() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr_commands" \
"*::: :->sailr" \
&& ret=0
    case $state in
    (sailr)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" \
'-c+[sailr config template path to use instead of the default one.]:Config Template Path: ' \
'--config-template=[sailr config template path to use instead of the default one.]:Config Template Path: ' \
'-r+[Default registry to use for images]:Default Registry: ' \
'--registry=[Default registry to use for images]:Default Registry: ' \
'-i+[Template path for infrastruture templates]:Infrastructure Template: ' \
'--infra-templates=[Template path for infrastruture templates]:Infrastructure Template: ' \
'-r+[Region to use for the provider]:Region: ' \
'--region=[Region to use for the provider]:Region: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':name -- Name of the environment:' \
'::provider -- Provider to use:(local aws gcp)' \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':shell -- Shell to generate completions for:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(infra)
_arguments "${_arguments_options[@]}" \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr__infra_commands" \
"*::: :->infra" \
&& ret=0

    case $state in
    (infra)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-infra-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" \
'-r+[Default registry to use for images]:Default Registry: ' \
'--registry=[Default registry to use for images]:Default Registry: ' \
'-i+[Template path for infrastruture templates]:Infrastructure Template: ' \
'--infra-templates=[Template path for infrastruture templates]:Infrastructure Template: ' \
'-r+[Region to use for the provider]:Region: ' \
'--region=[Region to use for the provider]:Region: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':name -- Name of the environment:' \
'::provider -- Provider to use:(local aws gcp)' \
&& ret=0
;;
(apply)
_arguments "${_arguments_options[@]}" \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(destroy)
_arguments "${_arguments_options[@]}" \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" \
":: :_sailr__infra__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-infra-help-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(apply)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(destroy)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(deploy)
_arguments "${_arguments_options[@]}" \
'-c+[Kubernetes context to use]:context: ' \
'--context=[Kubernetes context to use]:context: ' \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(generate)
_arguments "${_arguments_options[@]}" \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-f+[Force all rooms to build, ignore the cache]:force:(true false)' \
'--force=[Force all rooms to build, ignore the cache]:force:(true false)' \
'-i+[rooms to ignore from the build of the environment]:ignore: ' \
'--ignore=[rooms to ignore from the build of the environment]:ignore: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(go)
_arguments "${_arguments_options[@]}" \
'-c+[Kubernetes context to use]:context: ' \
'--context=[Kubernetes context to use]:context: ' \
'-n+[Name of the environment]:name: ' \
'--name=[Name of the environment]:name: ' \
'-f+[Force all rooms to build, ignore the cache]:force:(true false)' \
'--force=[Force all rooms to build, ignore the cache]:force:(true false)' \
'-i+[rooms to ignore from the build of the environment]:ignore: ' \
'--ignore=[rooms to ignore from the build of the environment]:ignore: ' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" \
":: :_sailr__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(infra)
_arguments "${_arguments_options[@]}" \
":: :_sailr__help__infra_commands" \
"*::: :->infra" \
&& ret=0

    case $state in
    (infra)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-infra-command-$line[1]:"
        case $line[1] in
            (create)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(apply)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(destroy)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
        esac
    ;;
esac
;;
(deploy)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(generate)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(go)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_sailr_commands] )) ||
_sailr_commands() {
    local commands; commands=(
'init:Initialize a new project' \
'completions:Generate shell completions' \
'infra:Manage environments' \
'deploy:Deploy an environment' \
'generate:Generate an environment' \
'build:Build related projects' \
'go:Generate and deploy an environment' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr commands' commands "$@"
}
(( $+functions[_sailr__help__infra__apply_commands] )) ||
_sailr__help__infra__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help infra apply commands' commands "$@"
}
(( $+functions[_sailr__infra__apply_commands] )) ||
_sailr__infra__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra apply commands' commands "$@"
}
(( $+functions[_sailr__infra__help__apply_commands] )) ||
_sailr__infra__help__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help apply commands' commands "$@"
}
(( $+functions[_sailr__build_commands] )) ||
_sailr__build_commands() {
    local commands; commands=()
    _describe -t commands 'sailr build commands' commands "$@"
}
(( $+functions[_sailr__help__build_commands] )) ||
_sailr__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help build commands' commands "$@"
}
(( $+functions[_sailr__completions_commands] )) ||
_sailr__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sailr completions commands' commands "$@"
}
(( $+functions[_sailr__help__completions_commands] )) ||
_sailr__help__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help completions commands' commands "$@"
}
(( $+functions[_sailr__help__infra__create_commands] )) ||
_sailr__help__infra__create_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help infra create commands' commands "$@"
}
(( $+functions[_sailr__infra__create_commands] )) ||
_sailr__infra__create_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra create commands' commands "$@"
}
(( $+functions[_sailr__infra__help__create_commands] )) ||
_sailr__infra__help__create_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help create commands' commands "$@"
}
(( $+functions[_sailr__deploy_commands] )) ||
_sailr__deploy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr deploy commands' commands "$@"
}
(( $+functions[_sailr__help__deploy_commands] )) ||
_sailr__help__deploy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help deploy commands' commands "$@"
}
(( $+functions[_sailr__help__infra__destroy_commands] )) ||
_sailr__help__infra__destroy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help infra destroy commands' commands "$@"
}
(( $+functions[_sailr__infra__destroy_commands] )) ||
_sailr__infra__destroy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra destroy commands' commands "$@"
}
(( $+functions[_sailr__infra__help__destroy_commands] )) ||
_sailr__infra__help__destroy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help destroy commands' commands "$@"
}
(( $+functions[_sailr__generate_commands] )) ||
_sailr__generate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr generate commands' commands "$@"
}
(( $+functions[_sailr__help__generate_commands] )) ||
_sailr__help__generate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help generate commands' commands "$@"
}
(( $+functions[_sailr__go_commands] )) ||
_sailr__go_commands() {
    local commands; commands=()
    _describe -t commands 'sailr go commands' commands "$@"
}
(( $+functions[_sailr__help__go_commands] )) ||
_sailr__help__go_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help go commands' commands "$@"
}
(( $+functions[_sailr__help_commands] )) ||
_sailr__help_commands() {
    local commands; commands=(
'init:Initialize a new project' \
'completions:Generate shell completions' \
'infra:Manage environments' \
'deploy:Deploy an environment' \
'generate:Generate an environment' \
'build:Build related projects' \
'go:Generate and deploy an environment' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr help commands' commands "$@"
}
(( $+functions[_sailr__help__help_commands] )) ||
_sailr__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help help commands' commands "$@"
}
(( $+functions[_sailr__infra__help_commands] )) ||
_sailr__infra__help_commands() {
    local commands; commands=(
'create:' \
'apply:' \
'destroy:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr infra help commands' commands "$@"
}
(( $+functions[_sailr__infra__help__help_commands] )) ||
_sailr__infra__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help help commands' commands "$@"
}
(( $+functions[_sailr__help__infra_commands] )) ||
_sailr__help__infra_commands() {
    local commands; commands=(
'create:' \
'apply:' \
'destroy:' \
    )
    _describe -t commands 'sailr help infra commands' commands "$@"
}
(( $+functions[_sailr__infra_commands] )) ||
_sailr__infra_commands() {
    local commands; commands=(
'create:' \
'apply:' \
'destroy:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr infra commands' commands "$@"
}
(( $+functions[_sailr__help__init_commands] )) ||
_sailr__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help init commands' commands "$@"
}
(( $+functions[_sailr__init_commands] )) ||
_sailr__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr init commands' commands "$@"
}

if [ "$funcstack[1]" = "_sailr" ]; then
    _sailr "$@"
else
    compdef _sailr sailr
fi
