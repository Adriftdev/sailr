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
    _arguments "${_arguments_options[@]}" : \
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
_arguments "${_arguments_options[@]}" : \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-c+[sailr config template path to use instead of the default one.]:Config Template Path:_default' \
'--config-template=[sailr config template path to use instead of the default one.]:Config Template Path:_default' \
'-r+[Default registry to use for images]:Default Registry:_default' \
'--registry=[Default registry to use for images]:Default Registry:_default' \
'-p+[Provider to use]:PROVIDER:(local aws gcp)' \
'--provider=[Provider to use]:PROVIDER:(local aws gcp)' \
'-i+[Template path for infrastruture templates]:Infrastructure Template:_default' \
'--infra-templates=[Template path for infrastruture templates]:Infrastructure Template:_default' \
'-R+[Region to use for the provider]:Region:_default' \
'--region=[Region to use for the provider]:Region:_default' \
'--env-type=[Environment type template to use]:ENV_TYPE:(development staging production)' \
'--with-sample[Include a sample service for immediate testing (default\: true)]' \
'(--with-sample)--no-sample[Skip creating sample service]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':shell -- Shell to generate completions for:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(infra)
_arguments "${_arguments_options[@]}" : \
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
            (up)
_arguments "${_arguments_options[@]}" : \
'-r+[Default registry to use for images]:Default Registry:_default' \
'--registry=[Default registry to use for images]:Default Registry:_default' \
'-i+[Template path for infrastruture templates]:Infrastructure Template:_default' \
'--infra-templates=[Template path for infrastruture templates]:Infrastructure Template:_default' \
'-r+[Region to use for the provider]:Region:_default' \
'--region=[Region to use for the provider]:Region:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':name -- Name of the environment:_default' \
'::provider -- Provider to use:(local aws gcp)' \
&& ret=0
;;
(down)
_arguments "${_arguments_options[@]}" : \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__infra__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-infra-help-command-$line[1]:"
        case $line[1] in
            (up)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(down)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
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
_arguments "${_arguments_options[@]}" : \
'-c+[Kubernetes context to use]:context:_default' \
'--context=[Kubernetes context to use]:context:_default' \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-N+[Namespace to deploy to]:namespace:_default' \
'--namespace=[Namespace to deploy to]:namespace:_default' \
'--strategy=[Deployment strategy to use]:STRATEGY:(restart rolling)' \
'--apply[Apply the deployment without planning first]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(generate)
_arguments "${_arguments_options[@]}" : \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-o+[]:ONLY:_default' \
'--only=[]:ONLY:_default' \
'-i+[]:IGNORE:_default' \
'--ignore=[]:IGNORE:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-f+[Force all rooms to build, ignore the cache]:force:(true false)' \
'--force=[Force all rooms to build, ignore the cache]:force:(true false)' \
'-i+[rooms to ignore from the build of the environment]:ignore:_default' \
'--ignore=[rooms to ignore from the build of the environment]:ignore:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(go)
_arguments "${_arguments_options[@]}" : \
'-c+[Kubernetes context to use]:context:_default' \
'--context=[Kubernetes context to use]:context:_default' \
'-n+[Name of the environment]:name:_default' \
'--name=[Name of the environment]:name:_default' \
'-N+[Namespace to deploy to]:namespace:_default' \
'--namespace=[Namespace to deploy to]:namespace:_default' \
'-i+[rooms to ignore from the build of the environment]:ignore:_default' \
'--ignore=[rooms to ignore from the build of the environment]:ignore:_default' \
'-o+[]:ONLY:_default' \
'--only=[]:ONLY:_default' \
'--strategy=[Deployment strategy to use for the deploy step]:STRATEGY:(restart rolling)' \
'-s[Skip the build step and run only generate and deploy steps]' \
'--skip-build[Skip the build step and run only generate and deploy steps]' \
'-f[Force all rooms to build, ignore the cache]' \
'--force[Force all rooms to build, ignore the cache]' \
'--apply[Apply the deployment without planning first]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(add-service)
_arguments "${_arguments_options[@]}" : \
'-t+[Type of the application (e.g., web-app, worker)]:APP_TYPE:_default' \
'--type=[Type of the application (e.g., web-app, worker)]:APP_TYPE:_default' \
'-p+[Port for the service (default is 80)]:PORT:_default' \
'--port=[Port for the service (default is 80)]:PORT:_default' \
'-i+[Docker image for the service (default is '\''nginx\:latest'\'')]:IMAGE:_default' \
'--image=[Docker image for the service (default is '\''nginx\:latest'\'')]:IMAGE:_default' \
'-n+[Environment to add the service to]:ENV_NAME:_default' \
'--name=[Environment to add the service to]:ENV_NAME:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':service_name -- Name of the service:_default' \
&& ret=0
;;
(interactive)
_arguments "${_arguments_options[@]}" : \
'-c+[Kubernetes context to use]:context:_default' \
'--context=[Kubernetes context to use]:context:_default' \
'-n+[Namespace to use]:namespace:_default' \
'--namespace=[Namespace to use]:namespace:_default' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
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
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(infra)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__help__infra_commands" \
"*::: :->infra" \
&& ret=0

    case $state in
    (infra)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-infra-command-$line[1]:"
        case $line[1] in
            (up)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(down)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(deploy)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(generate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(build)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(go)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(add-service)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(interactive)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
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
'add-service:Add a new service to the project' \
'interactive:Enter interactive terminal interface cli mode' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr commands' commands "$@"
}
(( $+functions[_sailr__add-service_commands] )) ||
_sailr__add-service_commands() {
    local commands; commands=()
    _describe -t commands 'sailr add-service commands' commands "$@"
}
(( $+functions[_sailr__build_commands] )) ||
_sailr__build_commands() {
    local commands; commands=()
    _describe -t commands 'sailr build commands' commands "$@"
}
(( $+functions[_sailr__completions_commands] )) ||
_sailr__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sailr completions commands' commands "$@"
}
(( $+functions[_sailr__deploy_commands] )) ||
_sailr__deploy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr deploy commands' commands "$@"
}
(( $+functions[_sailr__generate_commands] )) ||
_sailr__generate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr generate commands' commands "$@"
}
(( $+functions[_sailr__go_commands] )) ||
_sailr__go_commands() {
    local commands; commands=()
    _describe -t commands 'sailr go commands' commands "$@"
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
'add-service:Add a new service to the project' \
'interactive:Enter interactive terminal interface cli mode' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr help commands' commands "$@"
}
(( $+functions[_sailr__help__add-service_commands] )) ||
_sailr__help__add-service_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help add-service commands' commands "$@"
}
(( $+functions[_sailr__help__build_commands] )) ||
_sailr__help__build_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help build commands' commands "$@"
}
(( $+functions[_sailr__help__completions_commands] )) ||
_sailr__help__completions_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help completions commands' commands "$@"
}
(( $+functions[_sailr__help__deploy_commands] )) ||
_sailr__help__deploy_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help deploy commands' commands "$@"
}
(( $+functions[_sailr__help__generate_commands] )) ||
_sailr__help__generate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help generate commands' commands "$@"
}
(( $+functions[_sailr__help__go_commands] )) ||
_sailr__help__go_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help go commands' commands "$@"
}
(( $+functions[_sailr__help__help_commands] )) ||
_sailr__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help help commands' commands "$@"
}
(( $+functions[_sailr__help__infra_commands] )) ||
_sailr__help__infra_commands() {
    local commands; commands=(
'up:' \
'down:' \
    )
    _describe -t commands 'sailr help infra commands' commands "$@"
}
(( $+functions[_sailr__help__infra__down_commands] )) ||
_sailr__help__infra__down_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help infra down commands' commands "$@"
}
(( $+functions[_sailr__help__infra__up_commands] )) ||
_sailr__help__infra__up_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help infra up commands' commands "$@"
}
(( $+functions[_sailr__help__init_commands] )) ||
_sailr__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help init commands' commands "$@"
}
(( $+functions[_sailr__help__interactive_commands] )) ||
_sailr__help__interactive_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help interactive commands' commands "$@"
}
(( $+functions[_sailr__infra_commands] )) ||
_sailr__infra_commands() {
    local commands; commands=(
'up:' \
'down:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr infra commands' commands "$@"
}
(( $+functions[_sailr__infra__down_commands] )) ||
_sailr__infra__down_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra down commands' commands "$@"
}
(( $+functions[_sailr__infra__help_commands] )) ||
_sailr__infra__help_commands() {
    local commands; commands=(
'up:' \
'down:' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr infra help commands' commands "$@"
}
(( $+functions[_sailr__infra__help__down_commands] )) ||
_sailr__infra__help__down_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help down commands' commands "$@"
}
(( $+functions[_sailr__infra__help__help_commands] )) ||
_sailr__infra__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help help commands' commands "$@"
}
(( $+functions[_sailr__infra__help__up_commands] )) ||
_sailr__infra__help__up_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra help up commands' commands "$@"
}
(( $+functions[_sailr__infra__up_commands] )) ||
_sailr__infra__up_commands() {
    local commands; commands=()
    _describe -t commands 'sailr infra up commands' commands "$@"
}
(( $+functions[_sailr__init_commands] )) ||
_sailr__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr init commands' commands "$@"
}
(( $+functions[_sailr__interactive_commands] )) ||
_sailr__interactive_commands() {
    local commands; commands=()
    _describe -t commands 'sailr interactive commands' commands "$@"
}

if [ "$funcstack[1]" = "_sailr" ]; then
    _sailr "$@"
else
    compdef _sailr sailr
fi
