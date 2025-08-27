
# Copyright (C) Nginx, Inc.


ngx_addon_name=libnginx
ngx_module=$ngx_addon_name
ngx_module_c=$ngx_addon_dir/libnginx.c

ngx_ar="\$(AR)"
ngx_libext=.a
ngx_libout="r "

case "$NGX_CC_NAME" in

    msvc)
        ngx_ar=lib
        ngx_libext=.lib
        ngx_libout="/OUT:"
    ;;

esac

if test -n "$NGX_PCH"; then
    ngx_cc="\$(CC) $ngx_compile_opt \$(CFLAGS) $ngx_use_pch \$(ALL_INCS)"
else
    ngx_cc="\$(CC) $ngx_compile_opt \$(CFLAGS) \$(CORE_INCS)"
fi

ngx_module_objs=
for ngx_src in $ngx_module_c
do
    ngx_obj="addon/`basename \`dirname $ngx_src\``"

    test -d $NGX_OBJS/$ngx_obj || mkdir -p $NGX_OBJS/$ngx_obj

    ngx_obj=`echo $ngx_obj/\`basename $ngx_src\` \
        | sed -e "s/\//$ngx_regex_dirsep/g" \
              -e "s#^\(.*\.\)c\\$#$ngx_objs_dir\1$ngx_objext#g"`
    
    ngx_module_objs="$ngx_module_objs $ngx_obj"

    cat << END                                                >> $NGX_MAKEFILE

$ngx_obj:	\$(CORE_DEPS)$ngx_cont$ngx_src
	$ngx_cc$ngx_tab$ngx_objout$ngx_obj$ngx_tab$ngx_src$NGX_AUX

END

done

ngx_objs=`echo $ngx_module_objs $ngx_modules_obj $ngx_all_objs \
    | sed -e "s/[^ ]*\\/nginx\\.$ngx_objext//g" \
          -e "s/  *\([^ ][^ ]*\)/$ngx_long_regex_cont\1/g" \
          -e "s/\//$ngx_regex_dirsep/g"`

ngx_deps=`echo $ngx_module_objs $ngx_modules_obj $ngx_all_objs \
    | sed -e "s/[^ ]*\\/nginx\\.$ngx_objext//g" \
          -e "s/  *\([^ ][^ ]*\)/$ngx_regex_cont\1/g" \
          -e "s/\//$ngx_regex_dirsep/g"`

ngx_obj=$NGX_OBJS$ngx_dirsep$ngx_module$ngx_libext

cat << END                                                    >> $NGX_MAKEFILE

modules:	$ngx_obj

$ngx_obj:	$ngx_deps$ngx_spacer
	$ngx_ar $ngx_long_start$ngx_libout$ngx_obj$ngx_long_cont$ngx_objs
$ngx_long_end

LIBNGINX_LDFLAGS = $NGX_LD_OPT $CORE_LIBS

END
