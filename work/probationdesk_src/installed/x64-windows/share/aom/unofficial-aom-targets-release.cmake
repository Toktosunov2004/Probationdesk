#----------------------------------------------------------------
# Generated CMake target import file for configuration "Release".
#----------------------------------------------------------------

# Commands may need to know the format version.
set(CMAKE_IMPORT_FILE_VERSION 1)

# Import target "unofficial::aom" for configuration "Release"
set_property(TARGET unofficial::aom APPEND PROPERTY IMPORTED_CONFIGURATIONS RELEASE)
set_target_properties(unofficial::aom PROPERTIES
  IMPORTED_IMPLIB_RELEASE "${_IMPORT_PREFIX}/lib/aom_dll.lib"
  IMPORTED_LOCATION_RELEASE "${_IMPORT_PREFIX}/bin/aom.dll"
  )

list(APPEND _cmake_import_check_targets unofficial::aom )
list(APPEND _cmake_import_check_files_for_unofficial::aom "${_IMPORT_PREFIX}/lib/aom_dll.lib" "${_IMPORT_PREFIX}/bin/aom.dll" )

# Commands beyond this point should not need to know the version.
set(CMAKE_IMPORT_FILE_VERSION)
