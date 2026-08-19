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
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'--engine=[Build engine to configure]:ENGINE:(roomservice runkernel)' \
'--with-sample[Include a sample service for immediate testing (default\: true)]' \
'(--with-sample)--no-sample[Skip creating sample service]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(completions)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':shell -- Shell to generate completions for:(bash elvish fish powershell zsh)' \
&& ret=0
;;
(infra)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-R+[Region to use for the provider]:Region:_default' \
'--region=[Region to use for the provider]:Region:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'--only=[]:ONLY:_default' \
'-i+[rooms to ignore from the build of the environment]:ignore:_default' \
'--ignore=[rooms to ignore from the build of the environment]:ignore:_default' \
'--engine=[Build engine to use]:ENGINE:(roomservice runkernel)' \
'--plan[Plan the build without executing commands]' \
'--dry-run[Print the commands that would run without executing them]' \
'--explain[Explain why each room is dirty or clean]' \
'--dump-scope[Dump the resolved file scope for each room]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'--engine=[Build engine to use]:ENGINE:(roomservice runkernel)' \
'--strategy=[Deployment strategy to use for the deploy step]:STRATEGY:(restart rolling)' \
'-s[Skip the build step and run only generate and deploy steps]' \
'--skip-build[Skip the build step and run only generate and deploy steps]' \
'-f[Force all rooms to build, ignore the cache]' \
'--force[Force all rooms to build, ignore the cache]' \
'--plan[Plan the build step without executing commands]' \
'--dry-run[Print the build-step commands without executing them]' \
'--explain[Explain why each build room is dirty or clean]' \
'--dump-scope[Dump the resolved file scope for each room]' \
'--apply[Apply the deployment without planning first]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
'-e+[Sailr environment to use for interactive deploy]:environment:_default' \
'--environment=[Sailr environment to use for interactive deploy]:environment:_default' \
'-n+[Namespace to use]:namespace:_default' \
'--namespace=[Namespace to use]:namespace:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(migrate)
_arguments "${_arguments_options[@]}" : \
'-n+[]:NAME:_default' \
'--name=[]:NAME:_default' \
'--engine=[Build engine to configure after migration]:ENGINE:(roomservice runkernel)' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(bump)
_arguments "${_arguments_options[@]}" : \
'-n+[]:NAME:_default' \
'--name=[]:NAME:_default' \
'-s+[]:SERVICE:_default' \
'--service=[]:SERVICE:_default' \
'--version=[]:VERSION:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(lint)
_arguments "${_arguments_options[@]}" : \
'-n+[]:NAME:_default' \
'--name=[]:NAME:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(workflow)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr__workflow_commands" \
"*::: :->workflow" \
&& ret=0

    case $state in
    (workflow)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-workflow-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
'--environment=[Existing Sailr environment to use]:ENVIRONMENT:_default' \
'--preset=[Safe workflow profile preset]:PRESET:(build deploy portable-release)' \
'--context=[Kubernetes context for deploy profiles]:CONTEXT:_default' \
'--namespace=[Kubernetes namespace override]:NAMESPACE:_default' \
'--approval=[Portable-release approval mechanism]:APPROVAL:(external signature)' \
'--trusted-public-key-file=[File containing a base64-encoded raw Ed25519 public key]:TRUSTED_PUBLIC_KEY_FILE:_files' \
'--config=[Workflow configuration to create or update]:CONFIG:_files' \
'--print[Print the complete resulting configuration without writing it]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to create:_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(show)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to show:_default' \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
'--only=[]:ONLY:_default' \
'--ignore=[]:IGNORE:_default' \
'--release-id=[]:RELEASE_ID:_default' \
'--non-interactive[]' \
'--plan[]' \
'--dry-run[]' \
'--apply[]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to run:_default' \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
'--provider=[CI provider to generate for (github, circleci, travis)]:PROVIDER:_default' \
'--output=[Optional output file path]:OUTPUT:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to run:_default' \
&& ret=0
;;
(plan)
_arguments "${_arguments_options[@]}" : \
'--format=[]:FORMAT:(text json)' \
'--only=[]:ONLY:_default' \
'--ignore=[]:IGNORE:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to plan:_default' \
&& ret=0
;;
(graph)
_arguments "${_arguments_options[@]}" : \
'--format=[]:FORMAT:(text mermaid)' \
'--only=[]:ONLY:_default' \
'--ignore=[]:IGNORE:_default' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to graph:_default' \
&& ret=0
;;
(explain)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile:_default' \
':task -- ID of the task to explain:_default' \
&& ret=0
;;
(inspect)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile -- Name of the workflow profile to inspect:_default' \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
'--promotion-plan=[]:PROMOTION_PLAN:_files' \
'--out=[]:OUT:_files' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile:_default' \
&& ret=0
;;
(apply)
_arguments "${_arguments_options[@]}" : \
'--bundle=[]:BUNDLE:_files' \
'--release-id=[]:RELEASE_ID:_default' \
'--non-interactive[]' \
'--apply[]' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':profile:_default' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__workflow__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-workflow-help-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(show)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(plan)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(graph)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(explain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(inspect)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(apply)
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
(flow)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr__flow_commands" \
"*::: :->flow" \
&& ret=0

    case $state in
    (flow)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-flow-command-$line[1]:"
        case $line[1] in
            (inspect)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(validate)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
'--mode=[]:MODE:(print fragment create merge)' \
'--output=[]:OUTPUT:_files' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
'::flow -- Named flow; may be omitted when exactly one flow exists:_default' \
&& ret=0
;;
(check-release)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(check-gitops)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__flow__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-flow-help-command-$line[1]:"
        case $line[1] in
            (inspect)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(validate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check-release)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check-gitops)
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
(publication)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr__publication_commands" \
"*::: :->publication" \
&& ret=0

    case $state in
    (publication)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-publication-command-$line[1]:"
        case $line[1] in
            (validate)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
':report:_files' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__publication__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-publication-help-command-$line[1]:"
        case $line[1] in
            (validate)
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
(promote)
_arguments "${_arguments_options[@]}" : \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
":: :_sailr__promote_commands" \
"*::: :->promote" \
&& ret=0

    case $state in
    (promote)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-promote-command-$line[1]:"
        case $line[1] in
            (plan)
_arguments "${_arguments_options[@]}" : \
'(--from-manifest)*--from-report=[]:FROM_REPORTS:_files' \
'(--from-report)--from-manifest=[]:FROM_MANIFEST:_files' \
'--to=[]:TARGET_ENVIRONMENT:_default' \
'--out=[]:OUT:_files' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
'-h[Print help]' \
'--help[Print help]' \
'-V[Print version]' \
'--version[Print version]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__promote__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-promote-help-command-$line[1]:"
        case $line[1] in
            (plan)
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
(capabilities)
_arguments "${_arguments_options[@]}" : \
'--format=[]:FORMAT:(json)' \
'-q[Do not print log messages]' \
'--quiet[Do not print log messages]' \
'-v[Use verbose output]' \
'--verbose[Use verbose output]' \
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
(migrate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(bump)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(lint)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(workflow)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__help__workflow_commands" \
"*::: :->workflow" \
&& ret=0

    case $state in
    (workflow)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-workflow-command-$line[1]:"
        case $line[1] in
            (init)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(show)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(run)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(plan)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(graph)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(explain)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(inspect)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(prepare)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(apply)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(flow)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__help__flow_commands" \
"*::: :->flow" \
&& ret=0

    case $state in
    (flow)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-flow-command-$line[1]:"
        case $line[1] in
            (inspect)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(validate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(generate-ci)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check-release)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(check-gitops)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(publication)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__help__publication_commands" \
"*::: :->publication" \
&& ret=0

    case $state in
    (publication)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-publication-command-$line[1]:"
        case $line[1] in
            (validate)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(promote)
_arguments "${_arguments_options[@]}" : \
":: :_sailr__help__promote_commands" \
"*::: :->promote" \
&& ret=0

    case $state in
    (promote)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:sailr-help-promote-command-$line[1]:"
        case $line[1] in
            (plan)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(capabilities)
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
'migrate:Migrate an environment configuration to schema 0.5.0' \
'bump:Bump the version of a service' \
'lint:Lint an environment configuration' \
'workflow:Manage workflow profiles' \
'flow:Delivery flow management and validation' \
'publication:Validate immutable publication reports' \
'promote:Plan immutable artifact promotion' \
'capabilities:Show machine-readable Sailr feature support' \
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
(( $+functions[_sailr__bump_commands] )) ||
_sailr__bump_commands() {
    local commands; commands=()
    _describe -t commands 'sailr bump commands' commands "$@"
}
(( $+functions[_sailr__capabilities_commands] )) ||
_sailr__capabilities_commands() {
    local commands; commands=()
    _describe -t commands 'sailr capabilities commands' commands "$@"
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
(( $+functions[_sailr__flow_commands] )) ||
_sailr__flow_commands() {
    local commands; commands=(
'inspect:Inspect repository and output machine-readable JSON' \
'validate:Validate TOML/YAML syntax, duplicate workflow names, missing references, mutable image refs' \
'generate-ci:Generate or merge CI configuration' \
'check-release:Validate production invariants' \
'check-gitops:Validate development invariants' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr flow commands' commands "$@"
}
(( $+functions[_sailr__flow__check-gitops_commands] )) ||
_sailr__flow__check-gitops_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow check-gitops commands' commands "$@"
}
(( $+functions[_sailr__flow__check-release_commands] )) ||
_sailr__flow__check-release_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow check-release commands' commands "$@"
}
(( $+functions[_sailr__flow__generate-ci_commands] )) ||
_sailr__flow__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow generate-ci commands' commands "$@"
}
(( $+functions[_sailr__flow__help_commands] )) ||
_sailr__flow__help_commands() {
    local commands; commands=(
'inspect:Inspect repository and output machine-readable JSON' \
'validate:Validate TOML/YAML syntax, duplicate workflow names, missing references, mutable image refs' \
'generate-ci:Generate or merge CI configuration' \
'check-release:Validate production invariants' \
'check-gitops:Validate development invariants' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr flow help commands' commands "$@"
}
(( $+functions[_sailr__flow__help__check-gitops_commands] )) ||
_sailr__flow__help__check-gitops_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help check-gitops commands' commands "$@"
}
(( $+functions[_sailr__flow__help__check-release_commands] )) ||
_sailr__flow__help__check-release_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help check-release commands' commands "$@"
}
(( $+functions[_sailr__flow__help__generate-ci_commands] )) ||
_sailr__flow__help__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help generate-ci commands' commands "$@"
}
(( $+functions[_sailr__flow__help__help_commands] )) ||
_sailr__flow__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help help commands' commands "$@"
}
(( $+functions[_sailr__flow__help__inspect_commands] )) ||
_sailr__flow__help__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help inspect commands' commands "$@"
}
(( $+functions[_sailr__flow__help__validate_commands] )) ||
_sailr__flow__help__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow help validate commands' commands "$@"
}
(( $+functions[_sailr__flow__inspect_commands] )) ||
_sailr__flow__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow inspect commands' commands "$@"
}
(( $+functions[_sailr__flow__validate_commands] )) ||
_sailr__flow__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr flow validate commands' commands "$@"
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
'migrate:Migrate an environment configuration to schema 0.5.0' \
'bump:Bump the version of a service' \
'lint:Lint an environment configuration' \
'workflow:Manage workflow profiles' \
'flow:Delivery flow management and validation' \
'publication:Validate immutable publication reports' \
'promote:Plan immutable artifact promotion' \
'capabilities:Show machine-readable Sailr feature support' \
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
(( $+functions[_sailr__help__bump_commands] )) ||
_sailr__help__bump_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help bump commands' commands "$@"
}
(( $+functions[_sailr__help__capabilities_commands] )) ||
_sailr__help__capabilities_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help capabilities commands' commands "$@"
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
(( $+functions[_sailr__help__flow_commands] )) ||
_sailr__help__flow_commands() {
    local commands; commands=(
'inspect:Inspect repository and output machine-readable JSON' \
'validate:Validate TOML/YAML syntax, duplicate workflow names, missing references, mutable image refs' \
'generate-ci:Generate or merge CI configuration' \
'check-release:Validate production invariants' \
'check-gitops:Validate development invariants' \
    )
    _describe -t commands 'sailr help flow commands' commands "$@"
}
(( $+functions[_sailr__help__flow__check-gitops_commands] )) ||
_sailr__help__flow__check-gitops_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help flow check-gitops commands' commands "$@"
}
(( $+functions[_sailr__help__flow__check-release_commands] )) ||
_sailr__help__flow__check-release_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help flow check-release commands' commands "$@"
}
(( $+functions[_sailr__help__flow__generate-ci_commands] )) ||
_sailr__help__flow__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help flow generate-ci commands' commands "$@"
}
(( $+functions[_sailr__help__flow__inspect_commands] )) ||
_sailr__help__flow__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help flow inspect commands' commands "$@"
}
(( $+functions[_sailr__help__flow__validate_commands] )) ||
_sailr__help__flow__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help flow validate commands' commands "$@"
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
(( $+functions[_sailr__help__lint_commands] )) ||
_sailr__help__lint_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help lint commands' commands "$@"
}
(( $+functions[_sailr__help__migrate_commands] )) ||
_sailr__help__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help migrate commands' commands "$@"
}
(( $+functions[_sailr__help__promote_commands] )) ||
_sailr__help__promote_commands() {
    local commands; commands=(
'plan:Create a deterministic promotion plan' \
    )
    _describe -t commands 'sailr help promote commands' commands "$@"
}
(( $+functions[_sailr__help__promote__plan_commands] )) ||
_sailr__help__promote__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help promote plan commands' commands "$@"
}
(( $+functions[_sailr__help__publication_commands] )) ||
_sailr__help__publication_commands() {
    local commands; commands=(
'validate:Validate a workflow publication report' \
    )
    _describe -t commands 'sailr help publication commands' commands "$@"
}
(( $+functions[_sailr__help__publication__validate_commands] )) ||
_sailr__help__publication__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help publication validate commands' commands "$@"
}
(( $+functions[_sailr__help__workflow_commands] )) ||
_sailr__help__workflow_commands() {
    local commands; commands=(
'init:Create a workflow profile from an existing environment' \
'list:List available workflow profiles' \
'show:Show details of a workflow profile' \
'run:Run a workflow profile' \
'generate-ci:Generate a CI template for a workflow profile' \
'plan:Plan a workflow profile' \
'graph:View workflow graph' \
'explain:Explain a workflow task' \
'inspect:Inspect workflow diagnostic configuration' \
'prepare:Prepare and serialize an immutable deployment bundle' \
'apply:Apply a previously prepared immutable deployment bundle' \
    )
    _describe -t commands 'sailr help workflow commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__apply_commands] )) ||
_sailr__help__workflow__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow apply commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__explain_commands] )) ||
_sailr__help__workflow__explain_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow explain commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__generate-ci_commands] )) ||
_sailr__help__workflow__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow generate-ci commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__graph_commands] )) ||
_sailr__help__workflow__graph_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow graph commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__init_commands] )) ||
_sailr__help__workflow__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow init commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__inspect_commands] )) ||
_sailr__help__workflow__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow inspect commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__list_commands] )) ||
_sailr__help__workflow__list_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow list commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__plan_commands] )) ||
_sailr__help__workflow__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow plan commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__prepare_commands] )) ||
_sailr__help__workflow__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow prepare commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__run_commands] )) ||
_sailr__help__workflow__run_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow run commands' commands "$@"
}
(( $+functions[_sailr__help__workflow__show_commands] )) ||
_sailr__help__workflow__show_commands() {
    local commands; commands=()
    _describe -t commands 'sailr help workflow show commands' commands "$@"
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
(( $+functions[_sailr__lint_commands] )) ||
_sailr__lint_commands() {
    local commands; commands=()
    _describe -t commands 'sailr lint commands' commands "$@"
}
(( $+functions[_sailr__migrate_commands] )) ||
_sailr__migrate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr migrate commands' commands "$@"
}
(( $+functions[_sailr__promote_commands] )) ||
_sailr__promote_commands() {
    local commands; commands=(
'plan:Create a deterministic promotion plan' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr promote commands' commands "$@"
}
(( $+functions[_sailr__promote__help_commands] )) ||
_sailr__promote__help_commands() {
    local commands; commands=(
'plan:Create a deterministic promotion plan' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr promote help commands' commands "$@"
}
(( $+functions[_sailr__promote__help__help_commands] )) ||
_sailr__promote__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr promote help help commands' commands "$@"
}
(( $+functions[_sailr__promote__help__plan_commands] )) ||
_sailr__promote__help__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr promote help plan commands' commands "$@"
}
(( $+functions[_sailr__promote__plan_commands] )) ||
_sailr__promote__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr promote plan commands' commands "$@"
}
(( $+functions[_sailr__publication_commands] )) ||
_sailr__publication_commands() {
    local commands; commands=(
'validate:Validate a workflow publication report' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr publication commands' commands "$@"
}
(( $+functions[_sailr__publication__help_commands] )) ||
_sailr__publication__help_commands() {
    local commands; commands=(
'validate:Validate a workflow publication report' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr publication help commands' commands "$@"
}
(( $+functions[_sailr__publication__help__help_commands] )) ||
_sailr__publication__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr publication help help commands' commands "$@"
}
(( $+functions[_sailr__publication__help__validate_commands] )) ||
_sailr__publication__help__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr publication help validate commands' commands "$@"
}
(( $+functions[_sailr__publication__validate_commands] )) ||
_sailr__publication__validate_commands() {
    local commands; commands=()
    _describe -t commands 'sailr publication validate commands' commands "$@"
}
(( $+functions[_sailr__workflow_commands] )) ||
_sailr__workflow_commands() {
    local commands; commands=(
'init:Create a workflow profile from an existing environment' \
'list:List available workflow profiles' \
'show:Show details of a workflow profile' \
'run:Run a workflow profile' \
'generate-ci:Generate a CI template for a workflow profile' \
'plan:Plan a workflow profile' \
'graph:View workflow graph' \
'explain:Explain a workflow task' \
'inspect:Inspect workflow diagnostic configuration' \
'prepare:Prepare and serialize an immutable deployment bundle' \
'apply:Apply a previously prepared immutable deployment bundle' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr workflow commands' commands "$@"
}
(( $+functions[_sailr__workflow__apply_commands] )) ||
_sailr__workflow__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow apply commands' commands "$@"
}
(( $+functions[_sailr__workflow__explain_commands] )) ||
_sailr__workflow__explain_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow explain commands' commands "$@"
}
(( $+functions[_sailr__workflow__generate-ci_commands] )) ||
_sailr__workflow__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow generate-ci commands' commands "$@"
}
(( $+functions[_sailr__workflow__graph_commands] )) ||
_sailr__workflow__graph_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow graph commands' commands "$@"
}
(( $+functions[_sailr__workflow__help_commands] )) ||
_sailr__workflow__help_commands() {
    local commands; commands=(
'init:Create a workflow profile from an existing environment' \
'list:List available workflow profiles' \
'show:Show details of a workflow profile' \
'run:Run a workflow profile' \
'generate-ci:Generate a CI template for a workflow profile' \
'plan:Plan a workflow profile' \
'graph:View workflow graph' \
'explain:Explain a workflow task' \
'inspect:Inspect workflow diagnostic configuration' \
'prepare:Prepare and serialize an immutable deployment bundle' \
'apply:Apply a previously prepared immutable deployment bundle' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'sailr workflow help commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__apply_commands] )) ||
_sailr__workflow__help__apply_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help apply commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__explain_commands] )) ||
_sailr__workflow__help__explain_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help explain commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__generate-ci_commands] )) ||
_sailr__workflow__help__generate-ci_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help generate-ci commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__graph_commands] )) ||
_sailr__workflow__help__graph_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help graph commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__help_commands] )) ||
_sailr__workflow__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help help commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__init_commands] )) ||
_sailr__workflow__help__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help init commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__inspect_commands] )) ||
_sailr__workflow__help__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help inspect commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__list_commands] )) ||
_sailr__workflow__help__list_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help list commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__plan_commands] )) ||
_sailr__workflow__help__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help plan commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__prepare_commands] )) ||
_sailr__workflow__help__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help prepare commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__run_commands] )) ||
_sailr__workflow__help__run_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help run commands' commands "$@"
}
(( $+functions[_sailr__workflow__help__show_commands] )) ||
_sailr__workflow__help__show_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow help show commands' commands "$@"
}
(( $+functions[_sailr__workflow__init_commands] )) ||
_sailr__workflow__init_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow init commands' commands "$@"
}
(( $+functions[_sailr__workflow__inspect_commands] )) ||
_sailr__workflow__inspect_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow inspect commands' commands "$@"
}
(( $+functions[_sailr__workflow__list_commands] )) ||
_sailr__workflow__list_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow list commands' commands "$@"
}
(( $+functions[_sailr__workflow__plan_commands] )) ||
_sailr__workflow__plan_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow plan commands' commands "$@"
}
(( $+functions[_sailr__workflow__prepare_commands] )) ||
_sailr__workflow__prepare_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow prepare commands' commands "$@"
}
(( $+functions[_sailr__workflow__run_commands] )) ||
_sailr__workflow__run_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow run commands' commands "$@"
}
(( $+functions[_sailr__workflow__show_commands] )) ||
_sailr__workflow__show_commands() {
    local commands; commands=()
    _describe -t commands 'sailr workflow show commands' commands "$@"
}

if [ "$funcstack[1]" = "_sailr" ]; then
    _sailr "$@"
else
    compdef _sailr sailr
fi
