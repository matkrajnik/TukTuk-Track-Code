@echo off
rem Wydanie Todo Track jednym kliknieciem: pyta o numer wersji i haslo klucza,
rem potem robi wszystko jak `npm run ship` (RELEASE.md).
cd /d "%~dp0"
npm run ship
echo.
pause
