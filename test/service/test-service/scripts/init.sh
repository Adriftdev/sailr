#!/bin/bash
for i in $(seq 1 10); do
  echo $((1 + RANDOM % 10)) >> netrc
done
