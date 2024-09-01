echo START > start.txt
rem timeout /t 30 /nobreak
echo waiting
ping -n 30 -w 1000 localhost > NUL
echo DONE > done.txt
