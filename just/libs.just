import 'common.just'

set working-directory := ".."

org := "https://github.com/autistic-computer"

[default]
[private]
default:
    @just --list libs

[doc("Push Nidus's commits under libs/<lib> to autistic-computer/<lib> main.")]
push lib:
    git subtree push --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main

[doc("Merge autistic-computer/<lib> main into libs/<lib>.")]
pull lib:
    git subtree pull --prefix=libs/{{ lib }} {{ org }}/{{ lib }}.git main -m "Pull {{ lib }} from autistic-computer/{{ lib }}"
