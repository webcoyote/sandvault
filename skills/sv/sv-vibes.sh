#!/bin/bash
# Sandvault warp-into-the-vault — pure bash, no dependencies

P='\033[38;5;141m'
M='\033[38;5;213m'
H='\033[38;5;199m'
C='\033[38;5;117m'
G='\033[38;5;220m'
B='\033[38;5;69m'
W='\033[38;5;255m'
O='\033[38;5;208m'
SV='\033[38;5;247m'
LM='\033[38;5;46m'  # lime
AQ='\033[38;5;51m'  # aqua
DM='\033[2m'
BD='\033[1m'
IT='\033[3m'
R='\033[0m'

clear
sleep 0.2

# Phase 1: Starfield warp — stars get denser and streak
cols=$(tput cols 2>/dev/null || echo 80)
rows=$(tput lines 2>/dev/null || echo 24)
for frame in $(seq 1 6); do
  printf "\033[H"
  density=$(( 24 - frame * 4 ))
  [[ $density -lt 2 ]] && density=2
  for (( r=0; r<rows-2; r++ )); do
    line=""
    for (( c=0; c<cols; c++ )); do
      if (( RANDOM % density == 0 )); then
        case $(( RANDOM % 6 )) in
          0) line+="\033[38;5;141m" ;;
          1) line+="\033[38;5;213m" ;;
          2) line+="\033[38;5;117m" ;;
          3) line+="\033[38;5;220m" ;;
          4) line+="\033[38;5;69m" ;;
          5) line+="\033[38;5;208m" ;;
        esac
        if (( frame > 3 )); then
          line+="-"  # streaks at high speed
        else
          case $(( RANDOM % 4 )) in
            0) line+="." ;; 1) line+="*" ;; 2) line+="+" ;; 3) line+="." ;;
          esac
        fi
        line+="\033[0m"
      else
        line+=" "
      fi
    done
    printf "%b\n" "$line"
  done
  sleep 0.06
done

# Flash — two-color sweep
printf "\033[H"
for (( r=0; r<rows-2; r++ )); do
  if (( r % 2 == 0 )); then
    printf "${M}${BD}"
  else
    printf "${P}${BD}"
  fi
  printf '%*s' "$cols" '' | tr ' ' '/'
  printf "${R}\n"
done
sleep 0.05
printf "\033[H"
for (( r=0; r<rows-2; r++ )); do
  printf "${O}${BD}"
  printf '%*s' "$cols" '' | tr ' ' '#'
  printf "${R}\n"
done
sleep 0.04
clear
sleep 0.12

# Phase 2: Chrome SandVault text logo — typed in
echo ""
echo ""
echo -e "${BD}${H}        ____                  _${M} _    __          _  _   ${R}"
sleep 0.03
echo -e "${BD}${H}       / ___|  __ _ _ __   __| |${M}| |  / / __ _ _  _| || |_ ${R}"
sleep 0.03
echo -e "${BD}${H}       \\___ \\\\ / _\` | '_ \\\\ / _\` |${M}| | / / / _\` | | | | || __|${R}"
sleep 0.03
echo -e "${BD}${H}        ___) | (_| | | | | (_| |${M}| |/ / | (_| | |_| | ||_| ${R}"
sleep 0.03
echo -e "${BD}${H}       |____/ \\__,_|_| |_|\\__,_|${M}|___/   \\__,_|\\__,_|_| \\__|${R}"
sleep 0.1
echo ""

# Tagline typed out
printf "          ${DM}${C}${IT}"
tagline="seamlesssssly sandboxed agents on macOS"
for (( i=0; i<${#tagline}; i++ )); do
  printf "%s" "${tagline:$i:1}"
  sleep 0.02
done
printf "${R}\n"
echo ""
sleep 0.3

echo -e "   ${DM}${P}=======================================================${R}"
echo ""

# V = cursor jump to vault column
V='\033[43G'
# EL = erase entire line
EL='\033[2K'

# Frame 1: Star approaching from left
printf "\033[s"
echo -e "${EL}${V}${SV}${BD} _________${R}"
echo -e "${EL}       ${O}${BD}  *${R}${V}${SV}${BD}| VAULT   |${R}"
echo -e "${EL}       ${O}${BD} ***${R}${V}${SV}${BD}| [====]  |${R}"
echo -e "${EL}       ${O}${BD}*****${R}  ${DM}claude${R}${V}${SV}${BD}|  (o)    |${R}"
echo -e "${EL}       ${O}${BD} ***${R}${V}${SV}${BD}|_________|${R}"
echo -e "${EL}       ${O}${BD}  *${R}"
sleep 0.5

# Frame 2: Halfway with key
printf "\033[6A"
echo -e "${EL}${V}${SV}${BD} _________${R}"
echo -e "${EL}                    ${O}${BD}  *${R}${V}${SV}${BD}| VAULT   |${R}"
echo -e "${EL}                    ${O}${BD} ***${R}${V}${SV}${BD}| [====]  |${R}"
echo -e "${EL}                    ${O}${BD}*****${R} ${G}${BD}~key~${R}${V}${SV}${BD}|  (o)    |${R}"
echo -e "${EL}                    ${O}${BD} ***${R}${V}${SV}${BD}|_________|${R}"
echo -e "${EL}                    ${O}${BD}  *${R}"
sleep 0.4

# Frame 3: At the door
printf "\033[6A"
echo -e "${EL}${V}${SV}${BD} _________${R}"
echo -e "${EL}                              ${O}${BD}  *${R}${V}${SV}${BD}| VAULT   |${R}"
echo -e "${EL}                              ${O}${BD} ***${R}${V}${SV}${BD}| [====]  |${R}"
echo -e "${EL}                              ${O}${BD}*****${R} ${G}${BD}>${R}${V}${SV}${BD}|  (o)    |${R}"
echo -e "${EL}                              ${O}${BD} ***${R}${V}${SV}${BD}|_________|${R}"
echo -e "${EL}                              ${O}${BD}  *${R}"
sleep 0.3

# Frame 4: Star squeezes into vault
printf "\033[6A"
echo -e "${EL}${V}${SV}${BD} _________${R}"
echo -e "${EL}${V}${SV}${BD}|  ${O}${BD}       ${R}${SV}${BD}|${R}"
echo -e "${EL}${V}${SV}${BD}|  ${O}${BD}  *  ${R}${SV}${BD}  |${R}"
echo -e "${EL}${V}${SV}${BD}|  ${O}${BD} *** ${R}${SV}${BD}  |${R}"
echo -e "${EL}${V}${SV}${BD}|  ${O}${BD}  *  ${R}${SV}${BD}  |${R}"
echo -e "${EL}"
sleep 0.3

# Frame 5: Door slams shut
printf "\033[6A"
echo -e "${EL}${V}${SV}${BD} _________${R}"
echo -e "${EL}${V}${SV}${BD}| VAULT   |${R}"
echo -e "${EL}${V}${SV}${BD}| [====]  |${R}"
echo -e "${EL}${V}${G}${BD}|  (${O}*${G})     |${R}"
echo -e "${EL}${V}${SV}${BD}|_________|${R}"
echo -e "${EL}${V}${G}${BD}  LOCKED   ${R}"
sleep 0.3

# PARTY TIME — vault shakes, colors explode
pc=("\033[38;5;199m" "\033[38;5;213m" "\033[38;5;141m" "\033[38;5;220m" "\033[38;5;117m" "\033[38;5;208m" "\033[38;5;46m" "\033[38;5;51m")
words=("Securely" "Seamlessly" "Sandboxed" "Securely" "Seamlessly" "Sandboxed" "Securely" "Seamlessly" "Sandboxed" "Securely")
word2=("Dangerous" "Deployed" "Dangerous" "Delightful" "Dangerous" "Deployed" "Dangerous" "Delightful" "Dangerous" "Deployed")
word3=("Partytime" "Partytime!" "PARTYTIME" "Partytime" "PARTYTIME!" "Partytime" "PARTYTIME" "Partytime!" "PARTYTIME" "Partytime!")

for burst in $(seq 0 9); do
  printf "\033[6A"
  c1="${pc[$((RANDOM % 8))]}"
  c2="${pc[$((RANDOM % 8))]}"
  c3="${pc[$((RANDOM % 8))]}"

  w1="${words[$burst]}"
  w2="${word2[$burst]}"
  w3="${word3[$burst]}"

  case $((burst % 4)) in
    0) s1="   *     "; s2="  ***    "; s3="   *     " ;;
    1) s1="  * *    "; s2="   *     "; s3="  * *    " ;;
    2) s1="   *     "; s2=" *****   "; s3="   *     " ;;
    3) s1=" * * *   "; s2="  ***    "; s3=" * * *   " ;;
  esac

  echo -e "${EL}    ${c1}${BD}${w1}${R}${V}${c2}${BD} _________${R}"
  echo -e "${EL}    ${c2}${BD}${w2}${R}${V}${c3}${BD}| VAULT   |${R}"
  echo -e "${EL}    ${c3}${BD}${w3}${R}${V}${c1}${BD}|${O}${BD}${s1}${R}${c2}${BD}|${R}"
  echo -e "${EL}${V}${c3}${BD}|${O}${BD}${s2}${R}${c1}${BD}|${R}"
  echo -e "${EL}${V}${c2}${BD}|${O}${BD}${s3}${R}${c3}${BD}|${R}"
  echo -e "${EL}${V}${c1}${BD}|_________|${R}"
  sleep 0.12
done

# Final frame — settled
printf "\033[6A"
echo -e "${EL}    ${BD}${H}S${M}e${P}a${G}m${C}l${O}e${H}s${M}s${P}s${G}s${C}l${O}y${R}${V}${P}${BD} _________${R}"
echo -e "${EL}    ${BD}${P}S${G}a${C}n${O}d${H}b${M}o${P}x${G}e${C}d${R}${V}${P}${BD}| VAULT   |${R}"
echo -e "${EL}    ${BD}${O}A${H}g${M}e${P}n${G}t${C}s${R}${V}${P}${BD}|${O}${BD}   *     ${R}${P}${BD}|${R}"
echo -e "${EL}${V}${P}${BD}|${O}${BD} *****   ${R}${P}${BD}|${R}"
echo -e "${EL}${V}${P}${BD}|${O}${BD}   *     ${R}${P}${BD}|${R}"
echo -e "${EL}${V}${P}${BD}|_________|${R}"

sleep 0.3
echo ""

# Env info
REPO_NAME=$(basename "$(pwd)" 2>/dev/null || echo 'unknown')
BRANCH=$(git branch --show-current 2>/dev/null || echo 'n/a')
ORIGIN=$(git remote get-url origin 2>/dev/null || echo 'n/a')
SV_USER=$(whoami)
HOST_USER="${SV_USER#sandvault-}"
SANDBOX_HOME="$HOME"
SHARED_WS="${SHARED_WORKSPACE:-/Users/Shared/sv-${HOST_USER}}"
HAS_GH=$(command -v gh &>/dev/null && echo "yes" || echo "no")
DEPLOY_KEY_DIR="${SHARED_WS}/.ssh"
DEPLOY_KEY=$(ls "${DEPLOY_KEY_DIR}/deploy_${REPO_NAME}" 2>/dev/null && echo "yes" || echo "no")

echo -e "    ${DM}${P}=======================================================${R}"
echo -e "    ${DM}${C}host user:${R}      ${W}${BD}${HOST_USER}${R}"
echo -e "    ${DM}${C}sandbox user:${R}   ${W}${BD}${SV_USER}${R}"
echo -e "    ${DM}${C}sandbox home:${R}   ${W}${SANDBOX_HOME}${R}"
echo -e "    ${DM}${C}shared ws:${R}      ${W}${SHARED_WS}${R}"
echo -e "    ${DM}${C}repo:${R}           ${W}${BD}${REPO_NAME}${R}"
echo -e "    ${DM}${C}branch:${R}         ${W}${BD}${BRANCH}${R}"
echo -e "    ${DM}${C}origin:${R}         ${W}${ORIGIN}${R}"
echo -e "    ${DM}${C}deploy key:${R}     $([ "$DEPLOY_KEY" = "yes" ] && echo -e "${G}${BD}yes${R}" || echo -e "${DM}no${R}")"
echo -e "    ${DM}${C}gh cli:${R}         $([ "$HAS_GH" = "yes" ] && echo -e "${G}${BD}yes${R}" || echo -e "${DM}no${R}")"
echo -e "    ${DM}${C}perms:${R}          ${H}${BD}dangerously skipped${R}"
echo -e "    ${DM}${P}=======================================================${R}"
echo ""

# Random tip
tips=(
  "git push works — your deploy key is scoped to this repo only"
  "this sandbox can't touch your host files or SSH keys"
  "use 'git remote -v' to see your deploy key remote"
  "the sandbox user is $(whoami) — not your host account"
  "sv-clone reuses existing clones — just fetches on re-run"
  "your host repo has a 'sandvault' remote pointing here"
  "deploy keys are in \$SHARED_WORKSPACE/.ssh/"
  "permissions are dangerously skipped — the sandbox IS the boundary"
  "changes here don't affect your host repo until you push"
  "run /sv from Claude Code to hand off any task here"
)
tip="${tips[$((RANDOM % ${#tips[@]}))]}"
echo -e "    ${DM}${G}tip:${R} ${DM}${W}${tip}${R}"
echo ""
sleep 0.5
